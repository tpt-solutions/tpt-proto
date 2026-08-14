//! Interceptor / middleware model.
//!
//! Interceptors are composable, type-safe hooks applied around every RPC. They
//! receive the [`RpcContext`] before dispatch and may short-circuit with a
//! [`Status`] (e.g. authentication), inject metadata / extensions / deadlines,
//! or inspect the peer. They are applied at registration time by wrapping the
//! service handler, so the dispatch path stays uniform.

use std::sync::Arc;

use async_trait::async_trait;
use futures::future::BoxFuture;

use crate::context::RpcContext;
use crate::method::MethodKind;
use crate::service::ServiceHandler;
use crate::status::Status;

/// A composable, type-safe RPC interceptor.
///
/// Implementors run *before* the inner service handler. Returning `Err` aborts
/// the call with the given status; returning `Ok(ctx)` passes a (possibly
/// mutated) context to the handler.
#[async_trait]
pub trait Interceptor: Send + Sync + 'static {
    /// Inspect and possibly mutate the context, or short-circuit with a status.
    async fn intercept(&self, ctx: RpcContext, path: &str) -> Result<RpcContext, Status>;
}

/// A handler wrapper that applies a chain of [`Interceptor`]s around every call.
pub(crate) struct InterceptedHandler {
    inner: Arc<dyn ServiceHandler>,
    interceptors: Vec<Arc<dyn Interceptor>>,
}

impl InterceptedHandler {
    pub(crate) fn new(
        inner: Arc<dyn ServiceHandler>,
        interceptors: Vec<Arc<dyn Interceptor>>,
    ) -> Self {
        InterceptedHandler { inner, interceptors }
    }

    async fn apply(&self, ctx: RpcContext, path: &str) -> Result<RpcContext, Status> {
        let mut ctx = ctx;
        for ic in &self.interceptors {
            ctx = ic.intercept(ctx, path).await?;
        }
        Ok(ctx)
    }
}

#[async_trait]
impl ServiceHandler for InterceptedHandler {
    fn full_name(&self) -> &str {
        self.inner.full_name()
    }

    fn methods(&self) -> Vec<(String, MethodKind)> {
        self.inner.methods()
    }

    async fn call_unary(
        &self,
        method: &str,
        ctx: RpcContext,
        req: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        let path = format!("{}/{}", self.full_name(), method);
        let ctx = self.apply(ctx, &path).await?;
        self.inner.call_unary(method, ctx, req).await
    }

    async fn call_server_streaming(
        &self,
        method: &str,
        ctx: RpcContext,
        req: Vec<u8>,
    ) -> Result<crate::transport::ServerStream<Vec<u8>>, Status> {
        let path = format!("{}/{}", self.full_name(), method);
        let ctx = self.apply(ctx, &path).await?;
        self.inner.call_server_streaming(method, ctx, req).await
    }

    async fn call_client_streaming(
        &self,
        method: &str,
        ctx: RpcContext,
        req: crate::transport::ClientStream<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        let path = format!("{}/{}", self.full_name(), method);
        let ctx = self.apply(ctx, &path).await?;
        self.inner.call_client_streaming(method, ctx, req).await
    }

    async fn call_bidi_streaming(
        &self,
        method: &str,
        ctx: RpcContext,
        req: crate::transport::ClientStream<Vec<u8>>,
    ) -> Result<crate::transport::ServerStream<Vec<u8>>, Status> {
        let path = format!("{}/{}", self.full_name(), method);
        let ctx = self.apply(ctx, &path).await?;
        self.inner.call_bidi_streaming(method, ctx, req).await
    }
}

/// Convenience: build a simple interceptor from a closure.
///
/// The closure receives the context and path and returns (via a boxed future)
/// either the (possibly mutated) context or a terminal status.
pub fn from_fn<F, Fut>(f: F) -> Arc<dyn Interceptor>
where
    F: Fn(RpcContext, &str) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<RpcContext, Status>> + Send + 'static,
{
    struct ClosureInterceptor<F>(F);
    #[async_trait]
    impl<F, Fut> Interceptor for ClosureInterceptor<F>
    where
        F: Fn(RpcContext, &str) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<RpcContext, Status>> + Send + 'static,
    {
        async fn intercept(&self, ctx: RpcContext, path: &str) -> Result<RpcContext, Status> {
            (self.0)(ctx, path).await
        }
    }
    Arc::new(ClosureInterceptor(f))
}

/// Marker so `BoxFuture` import is always used regardless of feature set.
#[allow(dead_code)]
type _BoxFutureKeep = BoxFuture<'static, ()>;
