//! `tpt-proto-grpc` — gRPC protocol, server, and client runtime.
//!
//! Part of the clean-room, pure-Rust `tpt-proto` ecosystem.
//!
//! This crate implements the gRPC protocol layer on top of `tpt-proto`
//! message serialization: message framing, metadata, status/trailers,
//! deadlines & cancellation, compression, the four method kinds, and the RPC
//! context shared by generated server traits and client stubs.
//!
//! The HTTP/2 transport and streaming runtime (Phase 14) build on these
//! transport-agnostic types. This module set also provides Phase 15 building
//! blocks: observability (metrics/tracing/logging), the gRPC health-checking
//! protocol, server reflection, security configuration (TLS + auth/authz +
//! peer identity), and an HTTP/2 client [`Transport`] implementation.

pub mod cancellation;
pub mod codec;
pub mod compression;
pub mod context;
pub mod health;
pub mod metadata;
pub mod method;
pub mod observability;
pub mod reflection;
pub mod security;
pub mod status;
pub mod timeout;
pub mod transport;
pub mod transport_http2;

pub use cancellation::CancellationToken;
pub use codec::{
    decode_message, encode_message, COMPRESSION_FLAG_COMPRESSED, COMPRESSION_FLAG_UNCOMPRESSED,
    MAX_MESSAGE_SIZE_DEFAULT,
};
pub use compression::Compression;
pub use context::{Extensions, PeerInfo, Request, Response, RpcContext};
pub use health::{
    HealthCheckRequest, HealthCheckResponse, HealthRegistry, HealthService, ServingStatus,
};
pub use metadata::{Metadata, MAX_METADATA_SIZE_DEFAULT};
pub use method::{build_service, Method, MethodKind, Service};
pub use observability::{
    CallInstrumentor, CallLabels, InMemoryMetricsRecorder, JsonLogger, Level, LogRecord, Logger,
    LoggingTracer, MetricsRecorder, NoopLogger, NoopMetricsRecorder, NoopTracer, Observability,
    RequestIdLog, SpanContext, StreamingType, Tracer,
};
pub use reflection::{
    AllExtensionNumbersResponse, ErrorResponse, ExtensionRequest, FileDescriptorResponse,
    ListServiceResponse, ReflectionService, ServerReflectionRequest, ServerReflectionResponse,
    ServiceResponse,
};
pub use security::{
    AllowAllAuthorizer, Authenticator, Authorizer, BearerTokenAuthenticator, identity_from_peer,
    MetadataAuthenticator, parse_pem, PeerIdentity, PemBlock, RoleAuthorizer, SecurityError,
    SecurityPolicy, TlsConfig,
};
pub use status::{Code, Status};
pub use timeout::{format_timeout, parse_timeout};
pub use transport::{request, Channel, ClientStream, ServerStream, Transport};
pub use transport_http2::H2Transport;

/// Primary gRPC content type.
pub const CONTENT_TYPE_GRPC: &str = "application/grpc";
/// gRPC content type for protobuf-encoded payloads (explicit variant).
pub const CONTENT_TYPE_GRPC_PROTO: &str = "application/grpc+proto";
/// gRPC content type for JSON-encoded payloads (explicit variant).
pub const CONTENT_TYPE_GRPC_JSON: &str = "application/grpc+json";

/// Build the gRPC request path `/package.Service/Method`.
///
/// If `package` is empty the leading package segment is omitted, producing
/// `/Service/Method` as required by the gRPC wire spec.
pub fn build_path(package: &str, service: &str, method: &str) -> String {
    if package.is_empty() {
        format!("/{service}/{method}")
    } else {
        format!("/{package}.{service}/{method}")
    }
}

/// Parse a gRPC request path into `(package, service, method)`.
///
/// For `/pkg.sub.Service/Method` this yields `("pkg.sub", "Service", "Method")`;
/// for `/Service/Method` it yields `("", "Service", "Method")`.
pub fn parse_path(path: &str) -> Option<(String, String, String)> {
    let p = path.strip_prefix('/')?;
    let (svc, method) = p.rsplit_once('/')?;
    let (package, service) = match svc.rsplit_once('.') {
        Some((pkg, svc_name)) => (pkg.to_string(), svc_name.to_string()),
        None => ("".to_string(), svc.to_string()),
    };
    Some((package, service, method.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_roundtrip_with_package() {
        let path = build_path("example", "UserService", "GetUser");
        assert_eq!(path, "/example.UserService/GetUser");
        assert_eq!(
            parse_path(&path),
            Some(("example".into(), "UserService".into(), "GetUser".into()))
        );
    }

    #[test]
    fn path_roundtrip_without_package() {
        let path = build_path("", "UserService", "GetUser");
        assert_eq!(path, "/UserService/GetUser");
        assert_eq!(
            parse_path(&path),
            Some(("".into(), "UserService".into(), "GetUser".into()))
        );
    }

    #[test]
    fn path_with_nested_package() {
        assert_eq!(
            parse_path("/a.b.c.Service/Method"),
            Some(("a.b.c".into(), "Service".into(), "Method".into()))
        );
    }
}
