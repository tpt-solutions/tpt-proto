//! RPC context, peer information, request/response wrappers, and a small
//! type-keyed extension bag shared by generated server traits and clients.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, SystemTime};

use crate::cancellation::CancellationToken;
pub use crate::metadata::Metadata;
use crate::status::Status;

/// Information about the peer on the other end of an RPC.
#[derive(Debug, Clone, Default)]
pub struct PeerInfo {
    /// The peer's socket address, if known.
    pub peer_addr: Option<String>,
    /// The `user-agent` header value, if present.
    pub user_agent: Option<String>,
    /// An authenticated principal, if established by the transport.
    pub auth_principal: Option<String>,
}

/// A small type-keyed bag for attaching arbitrary values to an RPC context.
#[derive(Default)]
pub struct Extensions {
    map: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Extensions {
    /// Create an empty extension bag.
    pub fn new() -> Self {
        Extensions::default()
    }

    /// Insert a value, keyed by its type.
    pub fn insert<T: Any + Send + Sync>(&mut self, value: T) {
        self.map.insert(TypeId::of::<T>(), Box::new(value));
    }

    /// Fetch a reference to a value by type.
    pub fn get<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.map
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref::<T>())
    }

    /// Remove and return a value by type.
    pub fn remove<T: Any + Send + Sync>(&mut self) -> Option<Box<T>> {
        self.map
            .remove(&TypeId::of::<T>())
            .and_then(|b| b.downcast::<T>().ok())
    }

    /// Whether a value of the given type is present.
    pub fn contains<T: Any + Send + Sync>(&self) -> bool {
        self.map.contains_key(&TypeId::of::<T>())
    }
}

/// Per-call context exposing deadline, cancellation, metadata, peer, and
/// extensions. Constructed by the transport layer and threaded into handlers.
pub struct RpcContext {
    /// Incoming request metadata.
    pub metadata: Metadata,
    /// The call deadline, if a `grpc-timeout` was supplied.
    pub deadline: Option<SystemTime>,
    /// Cancellation token shared with the transport and downstream work.
    pub cancellation: CancellationToken,
    /// Peer information, if available from the transport.
    pub peer: Option<PeerInfo>,
    /// Arbitrary per-call extensions.
    pub extensions: Extensions,
}

impl RpcContext {
    /// Create a context with no deadline, a fresh cancellation token, and empty
    /// metadata/extensions.
    pub fn new() -> Self {
        RpcContext {
            metadata: Metadata::new(),
            deadline: None,
            cancellation: CancellationToken::new(),
            peer: None,
            extensions: Extensions::new(),
        }
    }

    /// Build a context with an explicit deadline computed from a timeout
    /// duration measured from now.
    pub fn with_timeout(timeout: Duration) -> Self {
        let mut c = RpcContext::new();
        c.deadline = Some(SystemTime::now() + timeout);
        c
    }

    /// Whether the call has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Signal cancellation for this call.
    pub fn cancel(&self) {
        self.cancellation.cancel();
    }

    /// The remaining time until the deadline, if any.
    pub fn remaining_time(&self) -> Option<Duration> {
        self.deadline.map(|d| {
            d.duration_since(SystemTime::now())
                .unwrap_or(Duration::ZERO)
        })
    }

    /// Whether the deadline has passed.
    pub fn is_expired(&self) -> bool {
        match self.deadline {
            Some(d) => SystemTime::now() >= d,
            None => false,
        }
    }
}

impl Default for RpcContext {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for RpcContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RpcContext")
            .field("deadline", &self.deadline)
            .field("peer", &self.peer)
            .field("cancelled", &self.cancellation.is_cancelled())
            .field("metadata_keys", &self.metadata.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// A request wrapper carrying the typed message plus its [`RpcContext`].
#[derive(Debug)]
pub struct Request<T> {
    /// The request message.
    pub message: T,
    /// The call context.
    pub context: RpcContext,
}

impl<T> Request<T> {
    /// Wrap a message in a request with a default context.
    pub fn new(message: T) -> Self {
        Request {
            message,
            context: RpcContext::new(),
        }
    }

    /// Wrap a message with an explicit context.
    pub fn with_context(message: T, context: RpcContext) -> Self {
        Request { message, context }
    }

    /// Consume the request, returning its message.
    pub fn into_inner(self) -> T {
        self.message
    }
}

/// A response wrapper carrying the typed message plus trailing metadata and an
/// optional terminal status.
#[derive(Debug)]
pub struct Response<T> {
    /// The response message.
    pub message: T,
    /// Trailing metadata.
    pub metadata: Metadata,
    /// The terminal status (defaults to [`Code::Ok`](crate::status::Code::Ok)
    /// when omitted).
    pub status: Option<Status>,
}

impl<T> Response<T> {
    /// Wrap a message in a successful response.
    pub fn new(message: T) -> Self {
        Response {
            message,
            metadata: Metadata::new(),
            status: None,
        }
    }

    /// Attach trailing metadata.
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Attach a terminal status.
    pub fn with_status(mut self, status: Status) -> Self {
        self.status = Some(status);
        self
    }

    /// Consume the response, returning its message.
    pub fn into_inner(self) -> T {
        self.message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_and_remaining() {
        let ctx = RpcContext::with_timeout(Duration::from_secs(10));
        assert!(!ctx.is_expired());
        assert!(ctx.remaining_time().unwrap() <= Duration::from_secs(10));
    }

    #[test]
    fn extensions_type_keyed() {
        let mut ext = Extensions::new();
        ext.insert(42u32);
        ext.insert("hello".to_string());
        assert_eq!(ext.get::<u32>(), Some(&42));
        assert_eq!(ext.get::<String>(), Some(&"hello".to_string()));
        assert!(ext.get::<i32>().is_none());
        assert!(ext.remove::<u32>().is_some());
        assert!(!ext.contains::<u32>());
    }

    #[test]
    fn request_response_wrappers() {
        let req = Request::new(7i32);
        assert_eq!(req.into_inner(), 7);
        let resp = Response::new("ok").with_metadata(Metadata::new());
        assert_eq!(resp.into_inner(), "ok");
    }
}
