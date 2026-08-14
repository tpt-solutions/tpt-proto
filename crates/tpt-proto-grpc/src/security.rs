//! Security: TLS configuration, authentication, authorization, and peer
//! identity inspection (§15, addendum §15).
//!
//! This module provides the *model* the server runtime applies during routing:
//!
//! * [`TlsConfig`] — cert/key/CA material plus ALPN protocols (must include
//!   `h2` for gRPC-over-HTTP/2) and mTLS toggles. The transport layer (Phase 14)
//!   performs the actual handshake and certificate validation from these
//!   settings.
//! * [`PeerIdentity`] — the authenticated principal and its attributes.
//! * [`Authenticator`] / [`Authorizer`] traits and concrete implementations
//!   (`BearerTokenAuthenticator`, `MetadataAuthenticator`, `AllowAllAuthorizer`)
//!   for token/metadata auth and authorization hooks.
//! * [`SecurityPolicy`] — composes an authenticator and an authorizer into a
//!   single `apply` the server calls before dispatching a handler.

use std::collections::HashMap;

use base64::Engine;
use thiserror::Error;

use crate::context::{PeerInfo, RpcContext};
use crate::metadata::Metadata;
use crate::status::{Code, Status};

/// The authenticated identity of a peer, produced by an [`Authenticator`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeerIdentity {
    /// The principal name (e.g. subject DN, username, or token subject).
    pub principal: String,
    /// Free-form attributes (e.g. roles, issuer, cert serial).
    pub attributes: HashMap<String, String>,
}

impl PeerIdentity {
    /// Construct an identity with just a principal.
    pub fn new(principal: impl Into<String>) -> Self {
        PeerIdentity {
            principal: principal.into(),
            attributes: HashMap::new(),
        }
    }

    /// Add an attribute, returning `self` for chaining.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Look up an attribute.
    pub fn attribute(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(|s| s.as_str())
    }
}

/// Errors from TLS configuration validation.
#[derive(Debug, Error)]
pub enum SecurityError {
    /// The certificate chain PEM block was missing or malformed.
    #[error("missing or malformed certificate chain: {0}")]
    MissingCertChain(String),
    /// The private key PEM block was missing or malformed.
    #[error("missing or malformed private key: {0}")]
    MissingPrivateKey(String),
    /// The client CA PEM block was missing or malformed (required for mTLS).
    #[error("missing or malformed client CA: {0}")]
    MissingClientCa(String),
    /// ALPN did not include `h2`.
    #[error("ALPN protocols must include \"h2\" for gRPC over HTTP/2")]
    MissingH2Alpn,
}

/// A single parsed PEM block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PemBlock {
    /// The label from the `-----BEGIN <label>-----` line.
    pub label: String,
    /// The decoded DER bytes.
    pub der: Vec<u8>,
}

/// Parse PEM-encoded data into its constituent blocks.
///
/// Each block is decoded from base64 between the `BEGIN`/`END` delimiters. This
/// is a minimal, dependency-free parser sufficient for loading cert/key/CA
/// material; full chain assembly and validation are performed by the transport.
pub fn parse_pem(data: &[u8]) -> Result<Vec<PemBlock>, SecurityError> {
    let text = String::from_utf8_lossy(data);
    let mut blocks = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let line = line.trim();
        if let Some(label) = line.strip_prefix("-----BEGIN ").and_then(|s| s.strip_suffix("-----")) {
            let mut b64 = String::new();
            for l in lines.by_ref() {
                let l = l.trim();
                if l.starts_with("-----END ") {
                    break;
                }
                b64.push_str(l);
            }
            let der = base64::engine::general_purpose::STANDARD
                .decode(b64.replace(['\r', '\n', ' ', '\t'], ""))
                .map_err(|e| SecurityError::MissingCertChain(e.to_string()))?;
            blocks.push(PemBlock {
                label: label.to_string(),
                der,
            });
        }
    }
    Ok(blocks)
}

/// TLS configuration consumed by the transport layer.
///
/// The actual handshake and certificate-chain validation happen in the
/// transport (Phase 14); this type carries the material and negotiation
/// parameters in a transport-agnostic form.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    /// The server certificate chain (PEM).
    pub cert_chain_pem: Vec<u8>,
    /// The server private key (PEM).
    pub private_key_pem: Vec<u8>,
    /// The client CA (PEM) for mTLS; `None` disables client-cert auth.
    pub client_ca_pem: Option<Vec<u8>>,
    /// Whether to require a client certificate (mutual TLS).
    pub require_client_cert: bool,
    /// ALPN protocols offered/accepted, in priority order. Must include `h2`.
    pub alpn_protocols: Vec<String>,
}

impl Default for TlsConfig {
    fn default() -> Self {
        TlsConfig {
            cert_chain_pem: Vec::new(),
            private_key_pem: Vec::new(),
            client_ca_pem: None,
            require_client_cert: false,
            alpn_protocols: vec!["h2".to_string()],
        }
    }
}

impl TlsConfig {
    /// Build a server TLS config from cert-chain and private-key PEM bytes.
    pub fn from_pem(cert_chain_pem: Vec<u8>, private_key_pem: Vec<u8>) -> Self {
        TlsConfig {
            cert_chain_pem,
            private_key_pem,
            ..Default::default()
        }
    }

    /// Enable mutual TLS by supplying a client CA (PEM) and requiring client
    /// certificates.
    pub fn with_client_ca(mut self, client_ca_pem: Vec<u8>) -> Self {
        self.client_ca_pem = Some(client_ca_pem);
        self.require_client_cert = true;
        self
    }

    /// Override the ALPN protocol list (must still include `h2`).
    pub fn with_alpn(mut self, protocols: Vec<String>) -> Self {
        self.alpn_protocols = protocols;
        self
    }

    /// Validate that the configuration is usable: PEM material present, mTLS
    /// has its CA, and `h2` is present in the ALPN list.
    pub fn validate(&self) -> Result<(), SecurityError> {
        if parse_pem(&self.cert_chain_pem)?.is_empty() {
            return Err(SecurityError::MissingCertChain(
                "no PEM block found".into(),
            ));
        }
        if parse_pem(&self.private_key_pem)?.is_empty() {
            return Err(SecurityError::MissingPrivateKey(
                "no PEM block found".into(),
            ));
        }
        if self.require_client_cert {
            match &self.client_ca_pem {
                None => return Err(SecurityError::MissingClientCa("mTLS requires a client CA".into())),
                Some(ca) if parse_pem(ca)?.is_empty() => {
                    return Err(SecurityError::MissingClientCa("no PEM block found".into()))
                }
                _ => {}
            }
        }
        if !self.alpn_protocols.iter().any(|p| p == "h2") {
            return Err(SecurityError::MissingH2Alpn);
        }
        Ok(())
    }

    /// The ALPN protocols as raw byte vectors (for `rustls`/`h2` negotiation).
    pub fn alpn_as_bytes(&self) -> Vec<Vec<u8>> {
        self.alpn_protocols.iter().map(|p| p.as_bytes().to_vec()).collect()
    }

    /// Whether mTLS (client-certificate authentication) is enabled.
    pub fn is_mutual(&self) -> bool {
        self.require_client_cert && self.client_ca_pem.is_some()
    }
}

/// A boxed authenticator.
pub trait Authenticator: Send + Sync {
    /// Authenticate an incoming request from its metadata and peer information.
    ///
    /// Returns the authenticated [`PeerIdentity`] or a [`Status`] describing the
    /// failure (typically [`Code::Unauthenticated`]).
    fn authenticate(
        &self,
        metadata: &Metadata,
        peer: &PeerInfo,
    ) -> Result<PeerIdentity, Status>;
}

/// Authenticate using a bearer token from the `authorization` header
/// (`Bearer <token>`), or from the `authorization-bin` binary metadata key.
///
/// The token is validated against the set of accepted tokens; the peer identity
/// principal is the matched token (or the `sub` attribute if provided).
#[derive(Debug, Clone, Default)]
pub struct BearerTokenAuthenticator {
    /// Accepted bearer tokens.
    accepted: std::collections::HashSet<String>,
    /// Map an accepted token to a principal name (optional).
    principal_for: HashMap<String, String>,
}

impl BearerTokenAuthenticator {
    /// Create an authenticator accepting the given token, mapped to `principal`.
    pub fn new(token: impl Into<String>, principal: impl Into<String>) -> Self {
        let token = token.into();
        let principal = principal.into();
        let mut a = BearerTokenAuthenticator::default();
        a.accepted.insert(token.clone());
        a.principal_for.insert(token, principal);
        a
    }

    /// Add an accepted token mapped to a principal.
    pub fn add_token(&mut self, token: impl Into<String>, principal: impl Into<String>) -> &mut Self {
        let token = token.into();
        let principal = principal.into();
        self.accepted.insert(token.clone());
        self.principal_for.insert(token, principal);
        self
    }
}

fn extract_bearer(metadata: &Metadata) -> Option<String> {
    if let Some(v) = metadata.get_text("authorization") {
        return v
            .strip_prefix("Bearer ")
            .or_else(|| v.strip_prefix("bearer "))
            .map(|t| t.trim().to_string());
    }
    if let Some(b) = metadata.get_binary("authorization-bin") {
        // authorization-bin is `Bearer <token>` in UTF-8 per the gRPC convention.
        let s = String::from_utf8_lossy(b);
        return s
            .strip_prefix("Bearer ")
            .or_else(|| s.strip_prefix("bearer "))
            .map(|t| t.trim().to_string());
    }
    None
}

impl Authenticator for BearerTokenAuthenticator {
    fn authenticate(&self, metadata: &Metadata, _peer: &PeerInfo) -> Result<PeerIdentity, Status> {
        let token = extract_bearer(metadata).ok_or_else(|| {
            Status::new(Code::Unauthenticated, "missing bearer token")
        })?;
        if !self.accepted.iter().any(|t| constant_time_eq(t.as_bytes(), token.as_bytes())) {
            return Err(Status::new(Code::Unauthenticated, "invalid bearer token"));
        }
        let principal = self
            .principal_for
            .iter()
            .find(|(t, _)| constant_time_eq(t.as_bytes(), token.as_bytes()))
            .map(|(_, p)| p.clone())
            .unwrap_or_else(|| token.clone());
        Ok(PeerIdentity::new(principal).with_attribute("auth", "bearer"))
    }
}

/// Authenticate by requiring a specific metadata key to equal an expected
/// value (e.g. an API key in `x-api-key`).
#[derive(Debug, Clone)]
pub struct MetadataAuthenticator {
    /// The metadata key to read.
    pub key: String,
    /// The accepted value (compared as UTF-8 text).
    pub expected: String,
    /// The principal name assigned on success.
    pub principal: String,
}

impl MetadataAuthenticator {
    /// Create a metadata-based authenticator.
    pub fn new(key: impl Into<String>, expected: impl Into<String>, principal: impl Into<String>) -> Self {
        MetadataAuthenticator {
            key: key.into(),
            expected: expected.into(),
            principal: principal.into(),
        }
    }
}

impl Authenticator for MetadataAuthenticator {
    fn authenticate(&self, metadata: &Metadata, _peer: &PeerInfo) -> Result<PeerIdentity, Status> {
        let value = metadata
            .get_text(&self.key)
            .ok_or_else(|| Status::new(Code::Unauthenticated, format!("missing {}", self.key)))?;
        if !constant_time_eq(value.as_bytes(), self.expected.as_bytes()) {
            return Err(Status::new(
                Code::Unauthenticated,
                format!("invalid {}", self.key),
            ));
        }
        Ok(PeerIdentity::new(self.principal.clone()).with_attribute("auth", "metadata"))
    }
}

/// Constant-time byte-slice comparison.
///
/// Compares two secrets without short-circuiting on the first differing byte,
/// preventing timing side channels that could otherwise leak token/key
/// contents. A length mismatch is folded into the accumulated difference so the
/// result is still independent of the *content* comparison (only the lengths,
/// which are generally not secret, remain observable).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    let n = a.len().min(b.len());
    for i in 0..n {
        diff |= (a[i] ^ b[i]) as usize;
    }
    diff == 0
}

/// Derive a peer identity purely from transport-level [`PeerInfo`] (e.g. a
/// verified client certificate subject populated by the TLS layer).
///
/// This is the fallback identity used when no explicit authenticator is
/// configured but peer inspection is desired.
pub fn identity_from_peer(peer: &PeerInfo) -> Option<PeerIdentity> {
    peer.auth_principal
        .clone()
        .map(|p| PeerIdentity::new(p).with_attribute("auth", "peer"))
}

/// A boxed authorizer.
pub trait Authorizer: Send + Sync {
    /// Authorize an already-authenticated identity to invoke
    /// `<service>/<method>`. Returns `Ok(())` or a [`Status`]
    /// ([`Code::PermissionDenied`] on denial).
    fn authorize(
        &self,
        identity: &PeerIdentity,
        service: &str,
        method: &str,
    ) -> Result<(), Status>;
}

/// An authorizer that allows everything (the default when no policy is set).
#[derive(Debug, Default, Clone, Copy)]
pub struct AllowAllAuthorizer;

impl Authorizer for AllowAllAuthorizer {
    fn authorize(&self, _identity: &PeerIdentity, _service: &str, _method: &str) -> Result<(), Status> {
        Ok(())
    }
}

/// A simple role/path-based authorizer.
///
/// Each rule maps a service path prefix (e.g. `ex.UserService` or
/// `ex.UserService/GetUser`) to the set of principals allowed to call it. A
/// missing rule defaults to allow (set [`deny_by_default`](RoleAuthorizer::deny_by_default)
/// to invert).
#[derive(Debug, Clone, Default)]
pub struct RoleAuthorizer {
    rules: HashMap<String, std::collections::HashSet<String>>,
    deny_by_default: bool,
}

impl RoleAuthorizer {
    /// Create an empty authorizer.
    pub fn new() -> Self {
        RoleAuthorizer::default()
    }

    /// Deny any call not explicitly allowed by a rule.
    pub fn deny_by_default(mut self) -> Self {
        self.deny_by_default = true;
        self
    }

    /// Allow `principal` to call `path` (service or `service/method`).
    pub fn allow(&mut self, path: impl Into<String>, principal: impl Into<String>) -> &mut Self {
        self.rules
            .entry(path.into())
            .or_default()
            .insert(principal.into());
        self
    }

    fn allowed(&self, path: &str, principal: &str) -> bool {
        if let Some(set) = self.rules.get(path) {
            return set.contains(principal);
        }
        // Try the service-only prefix.
        if let Some((svc, _)) = path.rsplit_once('/') {
            if let Some(set) = self.rules.get(svc) {
                return set.contains(principal);
            }
        }
        !self.deny_by_default
    }
}

impl Authorizer for RoleAuthorizer {
    fn authorize(&self, identity: &PeerIdentity, service: &str, method: &str) -> Result<(), Status> {
        let path = format!("{service}/{method}");
        if self.allowed(&path, &identity.principal) {
            Ok(())
        } else {
            Err(Status::new(
                Code::PermissionDenied,
                format!("principal '{}' may not call {}", identity.principal, path),
            ))
        }
    }
}

/// A composed security policy applied by the server before handler dispatch.
#[derive(Clone)]
pub struct SecurityPolicy {
    authenticator: Option<ArcAuthenticator>,
    authorizer: ArcAuthorizer,
}

type ArcAuthenticator = std::sync::Arc<dyn Authenticator>;
type ArcAuthorizer = std::sync::Arc<dyn Authorizer>;

impl std::fmt::Debug for SecurityPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecurityPolicy")
            .field("has_authenticator", &self.authenticator.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        SecurityPolicy {
            authenticator: None,
            authorizer: std::sync::Arc::new(AllowAllAuthorizer),
        }
    }
}

impl SecurityPolicy {
    /// A policy with no authentication and an allow-all authorizer.
    pub fn none() -> Self {
        SecurityPolicy::default()
    }

    /// Set the authenticator.
    pub fn with_authenticator(mut self, auth: ArcAuthenticator) -> Self {
        self.authenticator = Some(auth);
        self
    }

    /// Set the authorizer.
    pub fn with_authorizer(mut self, authz: ArcAuthorizer) -> Self {
        self.authorizer = authz;
        self
    }

    /// Apply the policy to an [`RpcContext`], returning the resolved
    /// [`PeerIdentity`] (or a default peer-derived identity when no
    /// authenticator is configured).
    ///
    /// Authentication and authorization failures are surfaced as [`Status`]
    /// (`UNAUTHENTICATED` / `PERMISSION_DENIED`).
    pub fn apply(&self, ctx: &RpcContext, service: &str, method: &str) -> Result<PeerIdentity, Status> {
        let identity = match &self.authenticator {
            Some(auth) => auth.authenticate(&ctx.metadata, ctx.peer.as_ref().unwrap_or(&PeerInfo::default()))?,
            None => match ctx.peer.as_ref().and_then(|p| identity_from_peer(p)) {
                Some(id) => id,
                None => PeerIdentity::default(),
            },
        };
        self.authorizer.authorize(&identity, service, method)?;
        Ok(identity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_validate_requires_h2() {
        let mut cfg = TlsConfig::from_pem(b"-----BEGIN CERTIFICATE-----\nMII=\n-----END CERTIFICATE-----".to_vec(),
            b"-----BEGIN PRIVATE KEY-----\nMII=\n-----END PRIVATE KEY-----".to_vec());
        cfg.alpn_protocols = vec!["http/1.1".to_string()];
        assert!(matches!(cfg.validate(), Err(SecurityError::MissingH2Alpn)));
    }

    #[test]
    fn tls_validate_mutual_requires_ca() {
        let cfg = TlsConfig::from_pem(
            b"-----BEGIN CERTIFICATE-----\nMII=\n-----END CERTIFICATE-----".to_vec(),
            b"-----BEGIN PRIVATE KEY-----\nMII=\n-----END PRIVATE KEY-----".to_vec(),
        )
        .with_client_ca(Vec::new());
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn tls_validate_ok() {
        let cfg = TlsConfig::from_pem(
            b"-----BEGIN CERTIFICATE-----\nMII=\n-----END CERTIFICATE-----".to_vec(),
            b"-----BEGIN PRIVATE KEY-----\nMII=\n-----END PRIVATE KEY-----".to_vec(),
        );
        assert!(cfg.validate().is_ok());
        assert_eq!(cfg.alpn_as_bytes(), vec![b"h2".to_vec()]);
    }

    #[test]
    fn parse_pem_roundtrip() {
        let data = b"-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----";
        let blocks = parse_pem(data).unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].label, "CERTIFICATE");
        assert_eq!(blocks[0].der, vec![0x30, 0x82, 0x01]);
    }

    #[test]
    fn bearer_auth_accepts_valid() {
        let auth = BearerTokenAuthenticator::new("secret", "alice");
        let mut md = Metadata::new();
        md.insert_text("authorization", "Bearer secret").unwrap();
        let id = auth.authenticate(&md, &PeerInfo::default()).unwrap();
        assert_eq!(id.principal, "alice");
    }

    #[test]
    fn bearer_auth_rejects_invalid() {
        let auth = BearerTokenAuthenticator::new("secret", "alice");
        let mut md = Metadata::new();
        md.insert_text("authorization", "Bearer wrong").unwrap();
        assert_eq!(
            auth.authenticate(&md, &PeerInfo::default()).unwrap_err().code,
            Code::Unauthenticated
        );
    }

    #[test]
    fn role_authorizer_enforces() {
        let mut authz = RoleAuthorizer::new().deny_by_default();
        authz.allow("ex.Svc/Get", "alice");
        let alice = PeerIdentity::new("alice");
        let bob = PeerIdentity::new("bob");
        assert!(authz.authorize(&alice, "ex.Svc", "Get").is_ok());
        assert_eq!(
            authz.authorize(&bob, "ex.Svc", "Get").unwrap_err().code,
            Code::PermissionDenied
        );
    }

    #[test]
    fn security_policy_applies() {
        let auth = std::sync::Arc::new(BearerTokenAuthenticator::new("t", "alice"));
        let authz = std::sync::Arc::new(AllowAllAuthorizer);
        let policy = SecurityPolicy::none()
            .with_authenticator(auth)
            .with_authorizer(authz);
        let mut ctx = RpcContext::new();
        ctx.metadata.insert_text("authorization", "Bearer t").unwrap();
        let id = policy.apply(&ctx, "ex.Svc", "Get").unwrap();
        assert_eq!(id.principal, "alice");

        let bad = RpcContext::new();
        assert_eq!(
            policy.apply(&bad, "ex.Svc", "Get").unwrap_err().code,
            Code::Unauthenticated
        );
    }
}
