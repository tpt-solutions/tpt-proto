//! Server-side TLS/mTLS termination via `rustls` (`tls` feature).
//!
//! This module bridges [`TlsConfig`] (transport-agnostic material +
//! parameters) to a real [`rustls::ServerConfig`], and provides
//! [`RustlsAcceptor`] — a [`StreamAcceptor`](crate::server::StreamAcceptor)
//! that performs the TLS handshake on every accepted TCP socket and extracts
//! the peer certificate's Subject CN / SAN DNS entries into a
//! [`PeerCertificate`] so authentication layers can derive principals.
//!
//! Handshake failures are logged through the configured
//! [`Observability`](crate::observability::Observability) logger instead of
//! being silently dropped.

use std::sync::Arc;

use rustls::pki_types::{
    CertificateDer, PrivateKeyDer, PrivatePkcs1KeyDer, PrivatePkcs8KeyDer, PrivateSec1KeyDer,
};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use x509_parser::prelude::*;

use crate::context::PeerCertificate;
use crate::observability::{Level, LogRecord, Logger, Observability};
use crate::security::{parse_pem, SecurityError, TlsConfig};
use crate::server::AcceptedStream;

/// Parse a PEM byte buffer into DER certificates.
fn parse_cert_chain(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>, SecurityError> {
    Ok(parse_pem(pem)?
        .into_iter()
        .filter(|b| b.label == "CERTIFICATE")
        .map(|b| CertificateDer::from(b.der))
        .collect())
}

/// Parse a PEM private key into a rustls-compatible key, honouring the three
/// standard labels (`PRIVATE KEY` = PKCS#8, `RSA PRIVATE KEY` = PKCS#1,
/// `EC PRIVATE KEY` = SEC1).
fn parse_private_key(pem: &[u8]) -> Result<PrivateKeyDer<'static>, SecurityError> {
    for block in parse_pem(pem)? {
        match block.label.as_str() {
            "PRIVATE KEY" => return Ok(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(block.der))),
            "RSA PRIVATE KEY" => {
                return Ok(PrivateKeyDer::Pkcs1(PrivatePkcs1KeyDer::from(block.der)))
            }
            "EC PRIVATE KEY" => return Ok(PrivateKeyDer::Sec1(PrivateSec1KeyDer::from(block.der))),
            _ => {}
        }
    }
    Err(SecurityError::MissingPrivateKey(
        "no supported private-key PEM block found".into(),
    ))
}

/// Build a real [`rustls::ServerConfig`] from a [`TlsConfig`]:
///
/// * parses the server certificate chain + private key and pairs them;
/// * when a client CA is present, installs a webpki client-certificate
///   verifier — *required* verification under mTLS
///   (`require_client_cert`) or optional-verification otherwise;
/// * copies the ALPN list.
///
/// Malformed or mismatched material surfaces as
/// [`SecurityError::InvalidTlsMaterial`] (or the more specific
/// missing-material variants), never as a deferred runtime error.
pub fn build_server_config(cfg: &TlsConfig) -> Result<Arc<rustls::ServerConfig>, SecurityError> {
    // Structural checks first so error codes stay precise (and so this is
    // safe to call without going through `TlsConfig::validate`).
    cfg.validate_structure()?;

    let certs = parse_cert_chain(&cfg.cert_chain_pem)?;
    let key = parse_private_key(&cfg.private_key_pem)?;

    let with_certs = if let Some(ca_pem) = &cfg.client_ca_pem {
        let mut roots = rustls::RootCertStore::empty();
        for cert in parse_cert_chain(ca_pem)? {
            roots.add(cert).map_err(|e| {
                SecurityError::InvalidTlsMaterial(format!("client CA unusable: {e}"))
            })?;
        }
        let verifier_builder = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots));
        let verifier = if cfg.require_client_cert {
            verifier_builder.build()
        } else {
            verifier_builder.allow_unauthenticated().build()
        };
        let verifier = verifier
            .map_err(|e| SecurityError::InvalidTlsMaterial(format!("client CA verifier: {e}")))?;
        rustls::ServerConfig::builder().with_client_cert_verifier(verifier)
    } else {
        rustls::ServerConfig::builder().with_no_client_auth()
    };

    let mut config = with_certs
        .with_single_cert(certs, key)
        .map_err(|e| SecurityError::InvalidTlsMaterial(format!("cert/key pair unusable: {e}")))?;
    config.alpn_protocols = cfg.alpn_as_bytes();
    Ok(Arc::new(config))
}

/// Extract the verified peer leaf certificate's CN/SAN attributes after a
/// completed TLS handshake.
fn extract_peer_certificate(conn: &rustls::ServerConnection) -> Option<PeerCertificate> {
    let certs = conn.peer_certificates()?;
    let leaf = certs.first()?;
    let der: Vec<u8> = leaf.as_ref().to_vec();

    let (_, cert) = X509Certificate::from_der(&der).ok()?;
    let subject_cn = cert
        .subject()
        .iter_common_name()
        .next()
        .and_then(|cn| cn.as_str().ok())
        .map(|s| s.to_string());
    let san_dns: Vec<String> = cert
        .extensions()
        .iter()
        .find_map(|ext| match ext.parsed_extension() {
            ParsedExtension::SubjectAlternativeName(san) => Some(san),
            _ => None,
        })
        .map(|san| {
            san.general_names
                .iter()
                .filter_map(|gn| match gn {
                    GeneralName::DNSName(d) => Some(d.to_string()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default();

    Some(PeerCertificate {
        subject_cn,
        san_dns,
        der,
    })
}

/// A cheaply cloneable [`StreamAcceptor`](crate::server::StreamAcceptor)
/// terminating TLS with a pre-built rustls configuration.
///
/// Construction validates the material eagerly: malformed cert/key/CA fail at
/// acceptor-build time rather than at the first inbound connection.
/// Per-connection handshake failures are logged via the configured
/// [`Observability`] logger (never silently dropped) and surface as
/// connection-level `io::Error`s.
#[derive(Clone)]
pub struct RustlsAcceptor {
    inner: Arc<RustlsAcceptorInner>,
}

struct RustlsAcceptorInner {
    tls: TlsAcceptor,
    observability: Observability,
}

impl RustlsAcceptor {
    /// Validate `config` and prepare the TLS acceptor with noop observability.
    pub fn new(config: &TlsConfig) -> Result<Self, SecurityError> {
        Self::with_observability(config, Observability::default())
    }

    /// Like [`RustlsAcceptor::new`], logging handshake failures through
    /// `observability`.
    pub fn with_observability(
        config: &TlsConfig,
        observability: Observability,
    ) -> Result<Self, SecurityError> {
        let rustls_config = build_server_config(config)?;
        Ok(RustlsAcceptor {
            inner: Arc::new(RustlsAcceptorInner {
                tls: TlsAcceptor::from(rustls_config),
                observability,
            }),
        })
    }

    fn log_handshake_failure(&self, peer: Option<std::net::SocketAddr>, err: &std::io::Error) {
        self.inner.observability.logger.log(&LogRecord {
            level: Level::Warn,
            message: format!("TLS handshake failed: {err}"),
            request_id: None,
            service: String::new(),
            method: String::new(),
            status: "tls_handshake_failed".to_string(),
            deadline: None,
            peer: peer.map(|p| p.to_string()),
            cancellation_reason: None,
            span_id: None,
        });
    }
}

impl crate::server::StreamAcceptor for RustlsAcceptor {
    fn accept(
        &self,
        stream: TcpStream,
    ) -> futures::future::BoxFuture<'static, std::io::Result<AcceptedStream>> {
        let peer_addr = stream.peer_addr().ok();
        let this = self.clone();
        Box::pin(async move {
            match this.inner.tls.accept(stream).await {
                Ok(tls_stream) => {
                    let peer_certificate = extract_peer_certificate(tls_stream.get_ref().1);
                    Ok(AcceptedStream {
                        stream: Box::new(tls_stream),
                        peer_certificate,
                    })
                }
                Err(e) => {
                    this.log_handshake_failure(peer_addr, &e);
                    Err(e)
                }
            }
        })
    }
}
