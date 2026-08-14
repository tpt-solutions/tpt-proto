//! Server-side service handler abstraction.
//!
//! Generated gRPC server traits (e.g. `UserService`) are plain `async` Rust
//! traits returning typed messages. The HTTP/2 runtime, however, operates on
//! raw serialized message bytes. [`ServiceHandler`] is the single seam that
//! bridges the two: codegen emits an `XxxServer<T>` wrapper implementing this
//! trait, decoding the request bytes into the typed message, dispatching to the
//! user's trait method, and re-encoding the response. The runtime then handles
//! all framing, compression, deadlines, and limits around it.

use async_trait::async_trait;

use crate::context::RpcContext;
use crate::method::MethodKind;
use crate::status::Status;
use crate::transport::{ClientStream, ServerStream};

/// A service handler that the server can route incoming calls to.
///
/// Implemented by generated `XxxServer<T>` wrappers. The runtime calls the
/// `call_*` method matching the method's [`MethodKind`] (discovered during
/// registration from [`ServiceHandler::methods`]).
#[async_trait]
pub trait ServiceHandler: Send + Sync {
    /// The fully-qualified service name (e.g. `example.UserService`).
    fn full_name(&self) -> &str;

    /// All routable methods as `(full_path, kind)` pairs. The server uses these
    /// to build its routing table and to know each method's call shape.
    fn methods(&self) -> Vec<(String, MethodKind)>;

    /// Handle a unary request. `req` is the raw (unframed) request message.
    async fn call_unary(
        &self,
        method: &str,
        ctx: RpcContext,
        req: Vec<u8>,
    ) -> Result<Vec<u8>, Status>;

    /// Handle a server-streaming request. Returns a stream of raw response
    /// messages.
    async fn call_server_streaming(
        &self,
        method: &str,
        ctx: RpcContext,
        req: Vec<u8>,
    ) -> Result<ServerStream<Vec<u8>>, Status>;

    /// Handle a client-streaming request. `req` is a stream of raw request
    /// messages; returns the single raw response message.
    async fn call_client_streaming(
        &self,
        method: &str,
        ctx: RpcContext,
        req: ClientStream<Vec<u8>>,
    ) -> Result<Vec<u8>, Status>;

    /// Handle a bidirectional-streaming request.
    async fn call_bidi_streaming(
        &self,
        method: &str,
        ctx: RpcContext,
        req: ClientStream<Vec<u8>>,
    ) -> Result<ServerStream<Vec<u8>>, Status>;
}

/// Helper: build a standard "method not found" status.
pub(crate) fn unimplemented_method(method: &str) -> Status {
    Status::new(
        crate::status::Code::Unimplemented,
        format!("method `{method}` is not implemented by this service"),
    )
}
