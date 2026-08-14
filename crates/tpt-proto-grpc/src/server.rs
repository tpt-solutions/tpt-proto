//! HTTP/2 gRPC server.
//!
//! The [`Server`] accepts TCP connections (cleartext HTTP/2 / "h2c" by default,
//! with a pluggable [`StreamAcceptor`] hook for TLS termination), performs the
//! h2 handshake, and dispatches each RPC through the registered
//! [`ServiceHandler`]s. It handles gRPC framing, compression negotiation,
//! metadata, deadlines/timeouts, message & metadata size limits, concurrent
//! stream limiting, graceful shutdown, and connection draining.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use futures::future::BoxFuture;
use futures::StreamExt;
use http::header::{CONTENT_TYPE, HeaderValue};
use http::{HeaderMap, Response};
use h2::server;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{RwLock, Semaphore};
use tokio::task::JoinSet;

use crate::cancellation::CancellationToken;
use crate::compression::Compression;
use crate::context::{PeerInfo, RpcContext};
use crate::framed::{deframe_stream, read_single_message};
use crate::interceptor::{InterceptedHandler, Interceptor};
use crate::metadata::Metadata;
use crate::method::MethodKind;
use crate::service::ServiceHandler;
use crate::status::{Code, Status};
use crate::timeout::parse_timeout;
use crate::transport::{ClientStream, ServerStream};

/// A combined read/write trait so a single trait object can be both.
pub trait TokioStream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> TokioStream for T {}

/// A boxed bidirectional byte stream (e.g. a TLS-wrapped socket).
pub type BoxedStream = Box<dyn TokioStream>;

/// A future produced by a [`StreamAcceptor`].
type AcceptFuture = BoxFuture<'static, std::io::Result<BoxedStream>>;

/// Strategy for turning an accepted [`TcpStream`] into the byte stream the h2
/// layer reads/writes.
///
/// The default [`CleartextAcceptor`] passes the socket through unchanged (h2c,
/// for local development). TLS termination can be layered by supplying an
/// acceptor that performs the TLS handshake (e.g. with `rustls`) before
/// returning the decrypted stream.
pub trait StreamAcceptor: Send + Sync + 'static {
    /// Transform `stream` into the stream h2 will use.
    fn accept(&self, stream: TcpStream) -> AcceptFuture;
}

/// Default acceptor: cleartext HTTP/2 ("h2c").
pub struct CleartextAcceptor;

impl StreamAcceptor for CleartextAcceptor {
    fn accept(&self, stream: TcpStream) -> AcceptFuture {
        Box::pin(async move { Ok(Box::new(stream) as BoxedStream) })
    }
}

/// Server tuning parameters.
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Maximum number of concurrently open HTTP/2 streams per connection.
    pub max_concurrent_streams: u32,
    /// Maximum size of a single gRPC message (unframed) in bytes.
    pub max_message_size: usize,
    /// Maximum total size of request metadata (headers) in bytes.
    pub max_metadata_size: usize,
    /// Maximum HTTP/2 header list size.
    pub max_header_list_size: u32,
    /// HTTP/2 initial stream flow-control window.
    pub http2_initial_stream_window: u32,
    /// HTTP/2 initial connection flow-control window.
    pub http2_initial_connection_window: u32,
    /// Compression applied to outgoing messages. Default `identity`.
    pub response_compression: Compression,
    /// Graceful-shutdown drain timeout after signalling GOAWAY.
    pub graceful_shutdown_timeout: Duration,
    /// Maximum number of concurrent in-flight RPCs across all connections.
    pub max_concurrent_rpcs: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            max_concurrent_streams: 100,
            max_message_size: 4 * 1024 * 1024,
            max_metadata_size: 8 * 1024,
            max_header_list_size: 16 * 1024,
            http2_initial_stream_window: 256 * 1024,
            http2_initial_connection_window: 1 * 1024 * 1024,
            response_compression: Compression::Identity,
            graceful_shutdown_timeout: Duration::from_secs(30),
            max_concurrent_rpcs: 1000,
        }
    }
}

/// A registered service: its handler plus the per-method call shape.
struct RegisteredService {
    handler: Arc<dyn ServiceHandler>,
    methods: HashMap<String, MethodKind>,
}

/// Builder for [`Server`].
pub struct ServerBuilder {
    config: ServerConfig,
    interceptors: Vec<Arc<dyn Interceptor>>,
}

impl Default for ServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerBuilder {
    /// Create a builder with default configuration.
    pub fn new() -> Self {
        ServerBuilder {
            config: ServerConfig::default(),
            interceptors: Vec::new(),
        }
    }

    /// Set the maximum permitted gRPC message size.
    pub fn max_message_size(mut self, size: usize) -> Self {
        self.config.max_message_size = size;
        self
    }

    /// Set the maximum permitted concurrent RPCs.
    pub fn max_concurrent_rpcs(mut self, n: usize) -> Self {
        self.config.max_concurrent_rpcs = n;
        self
    }

    /// Set the maximum HTTP/2 concurrent streams per connection.
    pub fn max_concurrent_streams(mut self, n: u32) -> Self {
        self.config.max_concurrent_streams = n;
        self
    }

    /// Set the compression algorithm used for responses.
    pub fn response_compression(mut self, c: Compression) -> Self {
        self.config.response_compression = c;
        self
    }

    /// Set the graceful-shutdown drain timeout.
    pub fn graceful_shutdown_timeout(mut self, d: Duration) -> Self {
        self.config.graceful_shutdown_timeout = d;
        self
    }

    /// Add a middleware interceptor applied to every RPC.
    pub fn with_interceptor(mut self, interceptor: Arc<dyn Interceptor>) -> Self {
        self.interceptors.push(interceptor);
        self
    }

    /// Build the [`Server`].
    pub fn build(self) -> Server {
        Server {
            config: self.config,
            interceptors: self.interceptors,
            registry: Arc::new(RwLock::new(HashMap::new())),
            shutdown: CancellationToken::new(),
        }
    }
}

/// A running gRPC server.
pub struct Server {
    config: ServerConfig,
    interceptors: Vec<Arc<dyn Interceptor>>,
    registry: Arc<RwLock<HashMap<String, RegisteredService>>>,
    shutdown: CancellationToken,
}

impl Default for Server {
    fn default() -> Self {
        Server::new()
    }
}

impl Server {
    /// Create a server with default configuration.
    pub fn new() -> Self {
        ServerBuilder::new().build()
    }

    /// Start building a server with custom configuration.
    pub fn builder() -> ServerBuilder {
        ServerBuilder::new()
    }

    /// Register a service handler, wiring every method into the routing table.
    ///
    /// The handler is wrapped with the server's interceptors (if any).
    pub fn add_service<S>(&self, service: S)
    where
        S: ServiceHandler + 'static,
    {
        let handler: Arc<dyn ServiceHandler> = Arc::new(service);
        let wrapped: Arc<dyn ServiceHandler> = if self.interceptors.is_empty() {
            handler.clone()
        } else {
            Arc::new(InterceptedHandler::new(
                handler.clone(),
                self.interceptors.clone(),
            ))
        };
        let mut registry = self.registry.blocking_write();
        for (path, kind) in handler.methods() {
            let mut methods = HashMap::new();
            methods.insert(path.clone(), kind);
            registry.insert(
                path,
                RegisteredService {
                    handler: wrapped.clone(),
                    methods,
                },
            );
        }
    }

    /// A clone of the cancellation token used to trigger graceful shutdown.
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    /// Begin graceful shutdown: stop accepting new connections and drain
    /// in-flight RPCs. Existing connections receive GOAWAY.
    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }

    /// Bind to `addr` and serve until [`Server::shutdown`] is called.
    pub async fn serve(self, addr: SocketAddr) -> std::io::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        self.serve_listener(listener).await
    }

    /// Serve incoming connections from an already-bound [`TcpListener`].
    pub async fn serve_listener(self, listener: TcpListener) -> std::io::Result<()> {
        self.serve_with_acceptor(listener, Arc::new(CleartextAcceptor))
            .await
    }

    /// Serve with a custom [`StreamAcceptor`] (e.g. for TLS termination).
    pub async fn serve_with_acceptor<A: StreamAcceptor>(
        self,
        listener: TcpListener,
        acceptor: Arc<A>,
    ) -> std::io::Result<()> {
        let mut conns: JoinSet<std::io::Result<()>> = JoinSet::new();
        let rpc_semaphore = Arc::new(Semaphore::new(self.config.max_concurrent_rpcs));

        loop {
            let accept_fut = listener.accept();
            tokio::select! {
                _ = self.shutdown.cancelled() => break,
                res = accept_fut => {
                    let (stream, peer) = match res {
                        Ok(v) => v,
                        Err(e) => {
                            eprintln!("grpc server: accept error: {e}");
                            continue;
                        }
                    };
                    let acceptor = acceptor.clone();
                    let registry = self.registry.clone();
                    let config = self.config.clone();
                    let shutdown = self.shutdown.clone();
                    let rpc_sem = rpc_semaphore.clone();
                    conns.spawn(async move {
                        let stream = acceptor.accept(stream).await?;
                        handle_connection(stream, peer, registry, config, shutdown, rpc_sem).await
                    });
                }
            }
        }

        let drain = async {
            while conns.join_next().await.is_some() {}
        };
        match tokio::time::timeout(self.config.graceful_shutdown_timeout, drain).await {
            Ok(_) => Ok(()),
            Err(_) => {
                conns.abort_all();
                Ok(())
            }
        }
    }
}

/// Drive a single h2 connection to completion.
async fn handle_connection(
    stream: BoxedStream,
    peer_addr: SocketAddr,
    registry: Arc<RwLock<HashMap<String, RegisteredService>>>,
    config: ServerConfig,
    shutdown: CancellationToken,
    rpc_semaphore: Arc<Semaphore>,
) -> std::io::Result<()> {
    let mut builder = server::Builder::new();
    builder
        .max_concurrent_streams(config.max_concurrent_streams)
        .max_header_list_size(config.max_header_list_size)
        .initial_window_size(config.http2_initial_stream_window)
        .initial_connection_window_size(config.http2_initial_connection_window);

    let mut conn = match builder.handshake::<_, Bytes>(stream).await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("grpc server: h2 handshake failed: {e}");
            return Ok(());
        }
    };

    loop {
        let next = tokio::select! {
            _ = shutdown.cancelled() => {
                conn.graceful_shutdown();
                None
            }
            r = conn.accept() => r,
        };
        match next {
            Some(Ok((request, respond))) => {
                let peer = peer_addr.to_string();
                let registry = registry.clone();
                let config = config.clone();
                let shutdown = shutdown.clone();
                let rpc_sem = rpc_semaphore.clone();
                tokio::spawn(async move {
                    let _permit = rpc_sem.acquire().await.ok();
                    handle_request(request, respond, peer, registry, config, shutdown).await;
                });
            }
            Some(Err(e)) => {
                eprintln!("grpc server: stream error: {e}");
            }
            None => break,
        }
    }
    let _ = conn; // connection is driven to completion by the accept loop above.
    Ok(())
}

/// Handle one incoming RPC.
async fn handle_request(
    request: http::Request<h2::RecvStream>,
    mut respond: h2::server::SendResponse<Bytes>,
    peer_addr: String,
    registry: Arc<RwLock<HashMap<String, RegisteredService>>>,
    config: ServerConfig,
    shutdown: CancellationToken,
) {
    let path = request.uri().path().to_string();
    let (handler, kind) = {
        let reg = registry.read().await;
        match reg.get(&path) {
            Some(svc) => {
                let kind = svc.methods.get(&path).copied().unwrap_or(MethodKind::Unary);
                (Some(svc.handler.clone()), kind)
            }
            None => (None, MethodKind::Unary),
        }
    };

    let (ctx, request_encoding) = build_context(&request, &peer_addr, config.max_metadata_size);

    let send_headers = |respond: &mut h2::server::SendResponse<Bytes>,
                         end: bool|
     -> Result<h2::SendStream<Bytes>, Status> {
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/grpc"),
        );
        let (mut parts, ()) = Response::new(()).into_parts();
        parts.headers = headers;
        respond
            .send_response(Response::from_parts(parts, ()), end)
            .map_err(|e| Status::new(Code::Internal, format!("send_response: {e}")))
    };

    let handler = match handler {
        Some(h) => h,
        None => {
            match send_headers(&mut respond, false) {
                Ok(mut send) => finish_with_status(&mut send, Status::new(Code::Unimplemented, format!("no method `{path}`"))).await,
                Err(e) => finish_with_status_respond(&mut respond, e).await,
            }
            return;
        }
    };

    let deadline_opt = ctx.deadline;
    let call_cancel = CancellationToken::new();
    let path_clone = path.clone();
    let work = dispatch(
        kind,
        request,
        handler,
        ctx,
        call_cancel,
        request_encoding,
        config.clone(),
        &path_clone,
    );

    let deadline_fut: Option<tokio::time::Sleep> = deadline_opt.map(|d| {
        let dur = d.duration_since(SystemTime::now()).unwrap_or(Duration::ZERO);
        tokio::time::sleep(dur)
    });

    let result: Result<HandlerOutput, Status> = match deadline_fut {
        Some(sleep) => tokio::select! {
            r = work => r,
            _ = shutdown.cancelled() => Err(Status::new(Code::Unavailable, "server shutting down")),
            _ = sleep => Err(Status::new(Code::DeadlineExceeded, "deadline exceeded")),
        },
        None => tokio::select! {
            r = work => r,
            _ = shutdown.cancelled() => Err(Status::new(Code::Unavailable, "server shutting down")),
        },
    };

    match result {
        Ok(output) => {
            let mut send = match send_headers(&mut respond, false) {
                Ok(s) => s,
                Err(e) => {
                    finish_with_status_respond(&mut respond, e).await;
                    return;
                }
            };
            if let Some(raw) = output.message {
                if let Err(e) = send_message(&mut send, &raw, &config) {
                    eprintln!("grpc server: send_message: {e}");
                    return;
                }
            }
            if let Some(mut stream) = output.stream {
                while let Some(item) = stream.next().await {
                    let raw = match item {
                        Ok(r) => r,
                        Err(e) => {
                            finish_with_status(&mut send, e).await;
                            return;
                        }
                    };
                    if let Err(e) = send_message(&mut send, &raw, &config) {
                        eprintln!("grpc server: send_message: {e}");
                        return;
                    }
                }
            }
            let mut trailers = output.status.to_trailers();
            trailers.insert_raw("grpc-status", output.status.code.as_i32().to_string().as_bytes());
            if !output.status.message.is_empty() {
                trailers.insert_raw("grpc-message", output.status.message.as_bytes());
            }
            if let Err(e) = send.send_trailers(trailers.to_header_map()) {
                eprintln!("grpc server: send_trailers: {e}");
            }
        }
        Err(status) => {
            match send_headers(&mut respond, false) {
                Ok(mut send) => finish_with_status(&mut send, status).await,
                Err(e) => finish_with_status_respond(&mut respond, e).await,
            }
        }
    }
}

/// Frame `raw` and send it as one message on the h2 send stream.
fn send_message(
    send: &mut h2::SendStream<Bytes>,
    raw: &[u8],
    config: &ServerConfig,
) -> Result<(), Status> {
    let framed = crate::codec::encode_message(
        raw,
        config.response_compression.clone(),
        config.max_message_size,
    )
    .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
    send.send_data(Bytes::from(framed), false)
        .map_err(|e| Status::new(Code::Internal, format!("send_data: {e}")))
}

/// Send a terminal status as trailers on an already-opened send stream.
async fn finish_with_status(send: &mut h2::SendStream<Bytes>, status: Status) {
    let mut trailers = status.to_trailers();
    trailers.insert_raw("grpc-status", status.code.as_i32().to_string().as_bytes());
    if !status.message.is_empty() {
        trailers.insert_raw("grpc-message", status.message.as_bytes());
    }
    if let Err(e) = send.send_trailers(trailers.to_header_map()) {
        eprintln!("grpc server: send_trailers: {e}");
    }
}

/// When response headers could not even be sent, attempt a trailers-only reply.
async fn finish_with_status_respond(
    respond: &mut h2::server::SendResponse<Bytes>,
    status: Status,
) {
    let mut trailers = status.to_trailers();
    trailers.insert_raw("grpc-status", status.code.as_i32().to_string().as_bytes());
    if !status.message.is_empty() {
        trailers.insert_raw("grpc-message", status.message.as_bytes());
    }
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/grpc"));
    let (mut parts, ()) = Response::new(()).into_parts();
    parts.headers = headers;
    match respond.send_response(Response::from_parts(parts, ()), false) {
        Ok(mut send) => {
            if let Err(e) = send.send_trailers(trailers.to_header_map()) {
                eprintln!("grpc server: send_trailers: {e}");
            }
        }
        Err(e) => eprintln!("grpc server: could not send trailers-only response: {e}"),
    }
}

/// The outcome of dispatching a single RPC.
struct HandlerOutput {
    /// For unary / client-streaming: the single response message.
    message: Option<Vec<u8>>,
    /// For server/bidi-streaming: the response stream.
    stream: Option<ServerStream<Vec<u8>>>,
    /// The terminal status (OK unless overridden).
    status: Status,
}

/// Dispatch to the right `call_*` based on the method kind.
async fn dispatch(
    kind: MethodKind,
    request: http::Request<h2::RecvStream>,
    handler: Arc<dyn ServiceHandler>,
    ctx: RpcContext,
    _call_cancel: CancellationToken,
    request_encoding: Compression,
    config: ServerConfig,
    path: &str,
) -> Result<HandlerOutput, Status> {
    let method = method_name(path)?;
    let max = config.max_message_size;
    match kind {
        MethodKind::Unary => {
            let body = request.into_body();
            let req = match read_single_message(body, request_encoding, max).await? {
                Some(r) => r,
                None => {
                    return Ok(HandlerOutput {
                        message: None,
                        stream: None,
                        status: Status::new(Code::InvalidArgument, "missing request message"),
                    })
                }
            };
            let raw = handler.call_unary(method, ctx, req).await?;
            Ok(HandlerOutput {
                message: Some(raw),
                stream: None,
                status: Status::ok(),
            })
        }
        MethodKind::ServerStreaming => {
            let body = request.into_body();
            let req = match read_single_message(body, request_encoding, max).await? {
                Some(r) => r,
                None => {
                    return Ok(HandlerOutput {
                        message: None,
                        stream: None,
                        status: Status::new(Code::InvalidArgument, "missing request message"),
                    })
                }
            };
            let stream = handler.call_server_streaming(method, ctx, req).await?;
            Ok(HandlerOutput {
                message: None,
                stream: Some(stream),
                status: Status::ok(),
            })
        }
        MethodKind::ClientStreaming => {
            let body = request.into_body();
            let req_stream: ClientStream<Vec<u8>> =
                Box::pin(deframe_stream(body, request_encoding, max).map(|r| r));
            let raw = handler.call_client_streaming(method, ctx, req_stream).await?;
            Ok(HandlerOutput {
                message: Some(raw),
                stream: None,
                status: Status::ok(),
            })
        }
        MethodKind::BidiStreaming => {
            let body = request.into_body();
            let req_stream: ClientStream<Vec<u8>> =
                Box::pin(deframe_stream(body, request_encoding, max).map(|r| r));
            let stream = handler.call_bidi_streaming(method, ctx, req_stream).await?;
            Ok(HandlerOutput {
                message: None,
                stream: Some(stream),
                status: Status::ok(),
            })
        }
    }
}

fn method_name(path: &str) -> Result<&str, Status> {
    path.rsplit_once('/')
        .map(|(_, m)| m)
        .ok_or_else(|| Status::new(Code::Internal, "malformed request path"))
}

/// Build an [`RpcContext`] from request headers.
fn build_context(
    request: &http::Request<h2::RecvStream>,
    peer_addr: &str,
    max_metadata_size: usize,
) -> (RpcContext, Compression) {
    let headers = request.headers();
    let mut metadata_pairs: Vec<(String, String)> = Vec::new();
    let mut deadline: Option<SystemTime> = None;
    let mut request_encoding = Compression::Identity;
    let mut user_agent: Option<String> = None;

    for (name, value) in headers.iter() {
        let name_str = name.as_str();
        if name_str.starts_with(':') || name_str == "content-type" || name_str == "te" {
            continue;
        }
        if let Ok(v) = value.to_str() {
            match name_str {
                "grpc-timeout" => {
                    if let Ok(d) = parse_timeout(v) {
                        deadline = Some(SystemTime::now() + d);
                    }
                }
                "grpc-encoding" => request_encoding = Compression::from_header(v),
                "user-agent" => user_agent = Some(v.to_string()),
                _ => metadata_pairs.push((name_str.to_string(), v.to_string())),
            }
        }
    }

    let metadata =
        Metadata::from_headers(&metadata_pairs, max_metadata_size).unwrap_or_default();

    let ctx = RpcContext {
        metadata,
        deadline,
        cancellation: CancellationToken::new(),
        peer: Some(PeerInfo {
            peer_addr: Some(peer_addr.to_string()),
            user_agent,
            auth_principal: None,
        }),
        extensions: Default::default(),
    };
    (ctx, request_encoding)
}
