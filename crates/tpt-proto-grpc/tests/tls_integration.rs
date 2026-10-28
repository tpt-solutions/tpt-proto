//! Server-side TLS/mTLS integration coverage (Phase 26 follow-up).
//!
//! Requires the `tls` feature (`cargo test -p tpt-proto-grpc --features tls`).
//!
//! Covered scenarios:
//!
//! * plain server-TLS round trip (real gRPC unary call over rustls),
//! * cleartext HTTP/2 client against a TLS-only server is rejected,
//! * mTLS-required server rejects a client presenting no certificate,
//! * full mTLS round trip where the verified client certificate's Subject CN
//!   feeds [`identity_from_peer`] and satisfies a deny-by-default
//!   [`RoleAuthorizer`],
//! * malformed TLS material fails fast at serve time (never at first
//!   connection).
//!
//! All certificates are generated in-process with `rcgen`; nothing touches
//! the filesystem.

#![cfg(feature = "tls")]

mod generated {
    include!("../examples/grpc_echo/echo_gen.rs");
}

use std::io::Cursor;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use generated::{Echo, EchoServer, ExPing, ExPong};
use http::{HeaderMap, HeaderName, HeaderValue};
use rustls::RootCertStore;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsConnector;

use tpt_proto_grpc::codec::{decode_message, encode_message};
use tpt_proto_grpc::compression::Compression;
use tpt_proto_grpc::metadata::Metadata;
use tpt_proto_grpc::status::{Code, Status};
use tpt_proto_grpc::{
    ClientStream, RoleAuthorizer, SecurityPolicy, Server, ServerStream, TlsConfig,
    CONTENT_TYPE_GRPC,
};

// ---------------------------------------------------------------------------
// Certificate material (rcgen, all in-memory)
// ---------------------------------------------------------------------------

/// Self-signed CA used to issue both server and client leaves for mTLS.
fn make_ca(cn: &str) -> (String, rcgen::KeyPair, rcgen::Certificate) {
    let mut params = rcgen::CertificateParams::default();
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, cn);
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap();
    let pem = cert.pem();
    (pem, key, cert)
}

/// A CA-signed leaf with the given CN + DNS SAN and both EKUs.
fn make_leaf(
    cn: &str,
    dns: &str,
    ca_cert: &rcgen::Certificate,
    ca_key: &rcgen::KeyPair,
) -> (Vec<u8>, Vec<u8>) {
    let mut params = rcgen::CertificateParams::new(vec![dns.to_string()]).unwrap();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, cn);
    params.extended_key_usages = vec![
        rcgen::ExtendedKeyUsagePurpose::ServerAuth,
        rcgen::ExtendedKeyUsagePurpose::ClientAuth,
    ];
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = params
        .signed_by(&key, ca_cert, ca_key)
        .expect("leaf signing failed");
    (cert.pem().into_bytes(), key.serialize_pem().into_bytes())
}

/// A real self-signed server material pair (SAN `localhost`).
fn self_signed_server() -> (Vec<u8>, Vec<u8>) {
    let mut params = rcgen::CertificateParams::new(vec!["localhost".to_string()]).unwrap();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, "test-server");
    params.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap();
    (cert.pem().into_bytes(), key.serialize_pem().into_bytes())
}

// ---------------------------------------------------------------------------
// Minimal TLS gRPC client transport (unary only)
// ---------------------------------------------------------------------------

struct TlsClient {
    addr: String,
    host: String,
    /// PEM CA bundle trusted to verify the server.
    root_pem: Vec<u8>,
    /// Optional client identity `(cert chain PEM, private key PEM)` for mTLS.
    identity: Option<(Vec<u8>, Vec<u8>)>,
}

impl TlsClient {
    fn tls_config(&self) -> Result<rustls::ClientConfig, Status> {
        let mut roots = RootCertStore::empty();
        for cert in rustls_pemfile::certs(&mut Cursor::new(&self.root_pem)) {
            let cert = cert.map_err(|e| Status::new(Code::Internal, format!("parse root: {e}")))?;
            roots
                .add(cert)
                .map_err(|e| Status::new(Code::Internal, format!("add root: {e}")))?;
        }
        let builder = rustls::ClientConfig::builder().with_root_certificates(roots);
        Ok(match &self.identity {
            None => builder.with_no_client_auth(),
            Some((cert_pem, key_pem)) => {
                let certs: Vec<_> = rustls_pemfile::certs(&mut Cursor::new(cert_pem))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| Status::new(Code::Internal, format!("client certs: {e}")))?;
                let key = rustls_pemfile::private_key(&mut Cursor::new(key_pem))
                    .map_err(|e| Status::new(Code::Internal, format!("client key: {e}")))?
                    .ok_or_else(|| Status::new(Code::Internal, "no client key in PEM"))?;
                builder
                    .with_client_auth_cert(certs, key)
                    .map_err(|e| Status::new(Code::Internal, format!("client auth: {e}")))?
            }
        })
    }

    async fn exchange(
        &self,
        path: &str,
        metadata: Metadata,
        framed: Vec<u8>,
    ) -> Result<(Vec<u8>, HeaderMap, Option<HeaderMap>), Status> {
        let tcp = TcpStream::connect(&self.addr)
            .await
            .map_err(|e| Status::new(Code::Unavailable, format!("connect: {e}")))?;
        let connector = TlsConnector::from(Arc::new(self.tls_config()?));
        let name = rustls::pki_types::ServerName::try_from(self.host.clone())
            .map_err(|e| Status::new(Code::Internal, format!("bad SNI name: {e}")))?;
        let tls = connector
            .connect(name, tcp)
            .await
            .map_err(|e| Status::new(Code::Unavailable, format!("tls handshake failed: {e}")))?;
        let (mut send, conn) = h2::client::Builder::new()
            .handshake(tls)
            .await
            .map_err(|e| Status::new(Code::Unavailable, format!("h2 handshake failed: {e}")))?;
        tokio::spawn(async move {
            let _ = conn.await;
        });

        let mut builder = http::Request::builder()
            .method("POST")
            .uri(format!("https://{}{}", self.addr, path))
            .header("content-type", CONTENT_TYPE_GRPC)
            .header("te", "trailers")
            .header("user-agent", "tpt-proto-grpc-tls-test/0.1");
        for (k, v) in metadata.to_headers() {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_bytes(v.as_bytes()),
            ) {
                builder = builder.header(name, val);
            }
        }
        let req = builder
            .body(())
            .map_err(|e| Status::new(Code::Internal, e.to_string()))?;

        let (resp_fut, mut body_send) = send
            .send_request(req, false)
            .map_err(|e| Status::new(Code::Unavailable, format!("send request: {e}")))?;
        body_send
            .send_data(Bytes::from(framed), true)
            .map_err(|e| Status::new(Code::Internal, format!("send data: {e}")))?;

        let resp = resp_fut
            .await
            .map_err(|e| Status::new(Code::Unavailable, format!("response: {e}")))?;
        let (parts, mut recv) = resp.into_parts();
        let mut body = Vec::new();
        while let Some(chunk) = recv.data().await {
            let chunk = chunk.map_err(|e| Status::new(Code::Internal, e.to_string()))?;
            body.extend_from_slice(&chunk);
        }
        let trailers = recv
            .trailers()
            .await
            .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        Ok((body, parts.headers, trailers))
    }

    async fn call_unary(&self, path: &str, ping: ExPing) -> Result<(ExPong, Metadata), Status> {
        let raw = ping.encode_to_vec().unwrap();
        let framed = encode_message(&raw, Compression::Identity, usize::MAX)
            .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        let (body, headers, trailers) = self.exchange(path, Metadata::new(), framed).await?;
        if body.len() < 5 {
            return Err(Status::new(Code::Internal, "truncated frame"));
        }
        let len = u32::from_be_bytes([body[1], body[2], body[3], body[4]]) as usize;
        let payload = decode_message(&body[..5 + len], Compression::Identity, usize::MAX)
            .map_err(|e| Status::new(Code::Internal, format!("decode frame: {e}")))?;
        let pong =
            ExPong::decode(&payload).map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        Ok((pong, merge_metadata(&headers, trailers.as_ref())))
    }
}

fn merge_metadata(headers: &HeaderMap, trailers: Option<&HeaderMap>) -> Metadata {
    // `grpc-status` and friends are reserved keys that only the
    // trailer-parsing path accepts, so round-trip through `from_trailers`.
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut add = |h: &HeaderMap| {
        for (name, value) in h.iter() {
            if name.as_str().starts_with(':') {
                continue;
            }
            if let Ok(v) = value.to_str() {
                pairs.push((name.as_str().to_string(), v.to_string()));
            }
        }
    };
    add(headers);
    if let Some(t) = trailers {
        add(t);
    }
    Metadata::from_trailers(&pairs, usize::MAX).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Echo service + server spawn harness
// ---------------------------------------------------------------------------

struct TimedEcho;

#[async_trait::async_trait]
impl Echo for TimedEcho {
    async fn unary(
        &self,
        request: tpt_proto_grpc::Request<ExPing>,
    ) -> Result<tpt_proto_grpc::Response<ExPong>, Status> {
        let mut p = ExPong::default();
        p.msg = format!("echo: {}", request.message.msg);
        Ok(tpt_proto_grpc::Response::new(p))
    }

    async fn server_stream(
        &self,
        _request: tpt_proto_grpc::Request<ExPing>,
    ) -> Result<tpt_proto_grpc::Response<ServerStream<ExPong>>, Status> {
        Err(Status::new(Code::Unimplemented, "not used here"))
    }

    async fn client_stream(
        &self,
        _request: tpt_proto_grpc::Request<ClientStream<ExPing>>,
    ) -> Result<tpt_proto_grpc::Response<ExPong>, Status> {
        Err(Status::new(Code::Unimplemented, "not used here"))
    }

    async fn bidi(
        &self,
        _request: tpt_proto_grpc::Request<ClientStream<ExPing>>,
    ) -> Result<tpt_proto_grpc::Response<ServerStream<ExPong>>, Status> {
        Err(Status::new(Code::Unimplemented, "not used here"))
    }
}

async fn spawn_tls_server(
    tls: TlsConfig,
    security: Option<Arc<SecurityPolicy>>,
) -> std::net::SocketAddr {
    let mut builder = Server::builder().with_tls(tls);
    if let Some(policy) = security {
        builder = builder.with_security(policy);
    }
    let server = builder.build();
    server.add_service(EchoServer::new(TimedEcho));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = server.serve_listener(listener).await;
    });
    addr
}

fn client_for(
    addr: std::net::SocketAddr,
    root_pem: Vec<u8>,
    identity: Option<(Vec<u8>, Vec<u8>)>,
) -> TlsClient {
    TlsClient {
        addr: addr.to_string(),
        host: "localhost".to_string(),
        root_pem,
        identity,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Plain server-side TLS: a client trusting the server certificate completes a
/// real unary RPC over rustls.
#[tokio::test]
async fn plain_server_tls_roundtrip() {
    let (cert, key) = self_signed_server();
    let addr = spawn_tls_server(TlsConfig::from_pem(cert.clone(), key), None).await;

    let client = client_for(addr, cert, None);
    let mut ping = ExPing::default();
    ping.msg = "over-tls".to_string();
    let (pong, meta) = client.call_unary("/ex.Echo/Unary", ping).await.unwrap();
    assert_eq!(pong.msg, "echo: over-tls");
    assert_eq!(meta.get_text("grpc-status"), Some("0"));
}

/// A cleartext HTTP/2 preface sent to a TLS-only server must be rejected: the
/// rustls handshake fails and the connection is torn down. The server may
/// first emit a TLS alert record (opaque bytes to a cleartext reader).
#[tokio::test]
async fn cleartext_client_is_rejected_by_tls_server() {
    let (cert, key) = self_signed_server();
    let addr = spawn_tls_server(TlsConfig::from_pem(cert, key), None).await;

    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n")
        .await
        .unwrap();
    let mut buf = [0u8; 32];
    loop {
        match tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buf)).await {
            Ok(Ok(0)) | Ok(Err(_)) => break, // torn down by the server.
            Ok(Ok(_)) => continue,           // alert bytes; keep draining to close.
            Err(_) => panic!("server kept the cleartext connection open"),
        }
    }
}

/// An mTLS-required server must reject a client that presents no certificate.
#[tokio::test]
async fn mtls_required_without_client_certificate_is_rejected() {
    let (ca_pem, ca_key, ca_cert) = make_ca("test-ca");
    let (server_pem, server_key) = make_leaf("test-server", "localhost", &ca_cert, &ca_key);

    let tls =
        TlsConfig::from_pem(server_pem, server_key).with_client_ca(ca_pem.clone().into_bytes());
    let addr = spawn_tls_server(tls, None).await;

    let client = client_for(addr, ca_pem.into_bytes(), None);
    let mut ping = ExPing::default();
    ping.msg = "should-fail".to_string();
    let err = client.call_unary("/ex.Echo/Unary", ping).await.unwrap_err();
    assert_eq!(err.code, Code::Unavailable, "got: {err}");
    // Under TLS 1.3 the client's handshake flight completes before the
    // server's `certificate_required` alert arrives, so the rejection may
    // surface either at connect time or on the first exchange.
    assert!(
        err.message.contains("handshake") || err.message.to_lowercase().contains("cert"),
        "got: {err}"
    );
}

/// Full mTLS round trip: the verified client certificate's CN becomes the
/// peer principal and satisfies a deny-by-default `RoleAuthorizer`.
#[tokio::test]
async fn full_mtls_roundtrip_identity_feeds_role_authorizer() {
    let (ca_pem, ca_key, ca_cert) = make_ca("test-ca");
    let (server_pem, server_key) = make_leaf("test-server", "localhost", &ca_cert, &ca_key);
    let (client_pem, client_key) =
        make_leaf("svc-client", "svc-client.internal", &ca_cert, &ca_key);

    let tls =
        TlsConfig::from_pem(server_pem, server_key).with_client_ca(ca_pem.clone().into_bytes());

    // Deny by default; only `svc-client` may call the echo unary method. The
    // server composes rules as `<service>/<method>` without a leading slash,
    // and the principal here can only come from the verified client
    // certificate (no token authenticator), proving cert-derived identity
    // feeds authorization.
    let mut authz = RoleAuthorizer::new().deny_by_default();
    authz.allow("ex.Echo/Unary", "svc-client");
    let policy = Arc::new(SecurityPolicy::none().with_authorizer(Arc::new(authz)));

    let addr = spawn_tls_server(tls, Some(policy)).await;

    let client = client_for(addr, ca_pem.into_bytes(), Some((client_pem, client_key)));
    let mut ping = ExPing::default();
    ping.msg = "mtls-ok".to_string();
    let (pong, _) = client.call_unary("/ex.Echo/Unary", ping).await.unwrap();
    assert_eq!(pong.msg, "echo: mtls-ok");
}

/// Malformed TLS material fails fast: both direct acceptor construction and
/// `serve_listener` reject it up front instead of at first connection.
#[tokio::test]
async fn malformed_config_fails_fast() {
    let (cert, _) = self_signed_server();
    let bogus_key =
        b"-----BEGIN PRIVATE KEY-----\nbm90IGEga2V5IGF0IGFsbA==\n-----END PRIVATE KEY-----";

    let cfg = TlsConfig::from_pem(cert, bogus_key.to_vec());

    // Direct acceptor construction surfaces the typed error.
    let err = match tpt_proto_grpc::RustlsAcceptor::new(&cfg) {
        Err(e) => e,
        Ok(_) => panic!("expected malformed material to be rejected"),
    };
    assert!(matches!(
        err,
        tpt_proto_grpc::SecurityError::InvalidTlsMaterial(_)
    ));

    // Serving with the same config fails immediately with an InvalidInput IO
    // error rather than accepting connections and failing lazily.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let result = Server::builder()
        .with_tls(cfg)
        .build()
        .serve_listener(listener)
        .await;
    let io_err = result.unwrap_err();
    assert_eq!(
        io_err.kind(),
        std::io::ErrorKind::InvalidInput,
        "got: {io_err}"
    );
}
