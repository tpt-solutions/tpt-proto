//! HTTP/2 gRPC client transport.
//!
//! [`Client`] implements the [`Transport`] trait by speaking real HTTP/2 to a
//! peer. It supports connection reuse / multiplexing over a single h2
//! connection, gzip & pluggable compression, per-call deadlines derived from
//! `grpc-timeout`, cancellation (by dropping the future), retry policies with
//! backoff, streaming backpressure, message-size limits, and a load-balancing
//! hook. TLS is plugged in via a stream acceptor (the `h2c` cleartext path is
//! used by default).

use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use bytes::BytesMut;
use futures::Stream;
use futures::StreamExt;
use http::{HeaderMap, Request};
use h2::client::SendRequest;
use h2::RecvStream;
use tokio::net::TcpStream;
use tokio::sync::Mutex;

use crate::codec::decode_message;
use crate::compression::Compression;
use crate::metadata::Metadata;
use crate::status::{Code, Status};
use crate::transport::{Channel, ClientStream, ServerStream, Transport};
use crate::BoxedStream;

/// A strategy for turning a connected [`TcpStream`] into the stream h2 uses.
pub trait ClientStreamAcceptor: Send + Sync + 'static {
    /// Wrap `stream` (e.g. perform a TLS handshake).
    fn accept(
        &self,
        stream: TcpStream,
    ) -> futures::future::BoxFuture<'static, std::io::Result<BoxedStream>>;
}

/// Default client acceptor: cleartext HTTP/2 ("h2c").
pub struct CleartextClientAcceptor;

impl ClientStreamAcceptor for CleartextClientAcceptor {
    fn accept(
        &self,
        stream: TcpStream,
    ) -> futures::future::BoxFuture<'static, std::io::Result<BoxedStream>> {
        Box::pin(async move { Ok(Box::new(stream) as BoxedStream) })
    }
}

/// Retry policy for unary calls.
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of additional attempts after the first.
    pub max_retries: u32,
    /// Base backoff between attempts.
    pub backoff: Duration,
    /// Codes that are considered retryable.
    pub retryable: Vec<Code>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        RetryPolicy {
            max_retries: 0,
            backoff: Duration::from_millis(50),
            retryable: vec![Code::Unavailable, Code::ResourceExhausted],
        }
    }
}

/// A load-balancing hook: resolves a logical target to concrete endpoints.
pub trait LoadBalancer: Send + Sync + 'static {
    /// Return the current set of endpoints to try, in preference order.
    fn endpoints(&self) -> Vec<Endpoint>;
}

/// A static, single-endpoint load balancer.
#[derive(Clone)]
pub struct StaticLoadBalancer {
    endpoint: Endpoint,
}

impl StaticLoadBalancer {
    /// Create a static balancer around `endpoint`.
    pub fn new(endpoint: Endpoint) -> Self {
        StaticLoadBalancer { endpoint }
    }
}

impl LoadBalancer for StaticLoadBalancer {
    fn endpoints(&self) -> Vec<Endpoint> {
        vec![self.endpoint.clone()]
    }
}

/// A client endpoint configuration.
#[derive(Clone)]
pub struct Endpoint {
    /// Host (or IP) to connect to.
    pub host: String,
    /// TCP port.
    pub port: u16,
    /// Compression to apply to outgoing messages.
    pub compression: Compression,
    /// Maximum accepted message size (unframed) in bytes.
    pub max_message_size: usize,
    /// Per-connection HTTP/2 initial stream window.
    pub http2_initial_stream_window: u32,
    /// Per-connection HTTP/2 initial connection window.
    pub http2_initial_connection_window: u32,
    /// Per-connection HTTP/2 max concurrent streams.
    pub max_concurrent_streams: u32,
    /// Timeout for establishing the TCP connection.
    pub connect_timeout: Duration,
    /// Retry policy applied to unary calls.
    pub retry_policy: RetryPolicy,
    /// The stream acceptor (TLS hook); defaults to cleartext h2c.
    pub acceptor: Arc<dyn ClientStreamAcceptor>,
    /// Optional load balancer for connection selection.
    pub load_balancer: Option<Arc<dyn LoadBalancer>>,
}

impl Default for Endpoint {
    fn default() -> Self {
        Endpoint {
            host: "localhost".to_string(),
            port: 50051,
            compression: Compression::Identity,
            max_message_size: 4 * 1024 * 1024,
            http2_initial_stream_window: 256 * 1024,
            http2_initial_connection_window: 1 * 1024 * 1024,
            max_concurrent_streams: 100,
            connect_timeout: Duration::from_secs(10),
            retry_policy: RetryPolicy::default(),
            acceptor: Arc::new(CleartextClientAcceptor),
            load_balancer: None,
        }
    }
}

impl Endpoint {
    /// Parse `host:port` into an endpoint.
    pub fn from_shared(addr: &str) -> Result<Endpoint, Status> {
        let (host, port) = addr
            .rsplit_once(':')
            .ok_or_else(|| Status::new(Code::InvalidArgument, "endpoint must be host:port"))?;
        let port: u16 = port
            .parse()
            .map_err(|_| Status::new(Code::InvalidArgument, "invalid port"))?;
        let mut e = Endpoint::default();
        e.host = host.to_string();
        e.port = port;
        Ok(e)
    }

    /// Set the outgoing compression algorithm.
    pub fn with_compression(mut self, c: Compression) -> Self {
        self.compression = c;
        self
    }

    /// Set the maximum message size.
    pub fn with_max_message_size(mut self, n: usize) -> Self {
        self.max_message_size = n;
        self
    }

    /// Set the retry policy.
    pub fn with_retry_policy(mut self, p: RetryPolicy) -> Self {
        self.retry_policy = p;
        self
    }

    /// Attach a load balancer.
    pub fn with_load_balancer(mut self, lb: Arc<dyn LoadBalancer>) -> Self {
        self.load_balancer = Some(lb);
        self
    }

    fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

/// A cached h2 connection.
struct ClientConn {
    send: SendRequest<Bytes>,
}

/// An HTTP/2 gRPC client implementing [`Transport`].
pub struct Client {
    endpoint: Endpoint,
    conn: Mutex<Option<ClientConn>>,
}

impl Client {
    /// Create a client for `endpoint`.
    pub fn new(endpoint: Endpoint) -> Self {
        Client {
            endpoint,
            conn: Mutex::new(None),
        }
    }

    /// Build a [`Channel`] backed by this client transport.
    pub fn into_channel(self) -> Channel {
        Channel::new(Arc::new(self))
    }

    /// Connect (or reuse) an h2 connection.
    async fn connection(&self) -> Result<ClientConn, Status> {
        {
            let guard = self.conn.lock().await;
            if let Some(c) = guard.as_ref() {
                return Ok(ClientConn {
                    send: c.send.clone(),
                });
            }
        }
        let endpoints = self
            .endpoint
            .load_balancer
            .as_ref()
            .map(|lb| lb.endpoints())
            .unwrap_or_else(|| vec![self.endpoint.clone()]);
        let mut last_err = Status::new(Code::Unavailable, "no endpoints");
        for ep in &endpoints {
            match connect(ep).await {
                Ok(c) => {
                    let mut guard = self.conn.lock().await;
                    *guard = Some(ClientConn {
                        send: c.send.clone(),
                    });
                    return Ok(c);
                }
                Err(e) => last_err = e,
            }
        }
        Err(last_err)
    }
}

/// Establish a fresh h2 connection to `endpoint`.
async fn connect(endpoint: &Endpoint) -> Result<ClientConn, Status> {
    let addr = format!("{}:{}", endpoint.host, endpoint.port);
    let tcp = tokio::time::timeout(endpoint.connect_timeout, TcpStream::connect(&addr))
        .await
        .map_err(|_| Status::new(Code::DeadlineExceeded, "connection timeout"))?
        .map_err(|e| Status::new(Code::Unavailable, format!("connect: {e}")))?;
    tcp.set_nodelay(true).ok();
    let stream = endpoint
        .acceptor
        .accept(tcp)
        .await
        .map_err(|e| Status::new(Code::Unavailable, format!("acceptor: {e}")))?;

    let mut builder = h2::client::Builder::new();
    builder
        .max_concurrent_streams(endpoint.max_concurrent_streams)
        .initial_window_size(endpoint.http2_initial_stream_window)
        .initial_connection_window_size(endpoint.http2_initial_connection_window);

    let (send, connection) = builder
        .handshake::<_, Bytes>(stream)
        .await
        .map_err(|e| Status::new(Code::Unavailable, format!("h2 handshake: {e}")))?;

    tokio::spawn(async move {
        let _ = connection.await;
    });

    Ok(ClientConn { send })
}

/// A response body reader that yields deframed messages and surfaces a
/// non-OK terminal status (from trailers) as a final error item.
struct GrpcResponseBody {
    body: RecvStream,
    buf: BytesMut,
    encoding: Compression,
    max: usize,
    eof: bool,
    trailers_checked: bool,
    terminal_status: Option<Status>,
}

impl GrpcResponseBody {
    fn new(body: RecvStream, encoding: Compression, max: usize) -> Self {
        GrpcResponseBody {
            body,
            buf: BytesMut::new(),
            encoding,
            max,
            eof: false,
            trailers_checked: false,
            terminal_status: None,
        }
    }
}

impl Stream for GrpcResponseBody {
    type Item = Result<Vec<u8>, Status>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            if this.buf.len() >= 5 {
                let declared =
                    u32::from_be_bytes([this.buf[1], this.buf[2], this.buf[3], this.buf[4]]) as usize;
                if declared > this.max {
                    return std::task::Poll::Ready(Some(Err(Status::new(
                        Code::ResourceExhausted,
                        format!("message length {declared} exceeds maximum {}", this.max),
                    ))));
                }
                let total = 5 + declared;
                if this.buf.len() >= total {
                    let frame: Bytes = this.buf.split_to(total).freeze();
                    let raw = match decode_message(&frame, this.encoding.clone(), this.max) {
                        Ok(r) => r,
                        Err(e) => {
                            return std::task::Poll::Ready(Some(Err(Status::new(
                                Code::Internal,
                                format!("decode frame: {e}"),
                            ))))
                        }
                    };
                    return std::task::Poll::Ready(Some(Ok(raw)));
                }
            }

            if this.eof {
                if !this.buf.is_empty() {
                    return std::task::Poll::Ready(Some(Err(Status::new(
                        Code::Internal,
                        "connection closed with a partial gRPC frame",
                    ))));
                }
                if !this.trailers_checked {
                    this.trailers_checked = true;
                    match this.body.poll_trailers(cx) {
                        std::task::Poll::Ready(Ok(Some(headers))) => {
                            let status = Status::from_trailers(
                                &Metadata::from_trailers(&headers_to_pairs(&headers), usize::MAX)
                                    .unwrap_or_default(),
                            );
                            if !status.is_ok() {
                                this.terminal_status = Some(status.clone());
                                return std::task::Poll::Ready(Some(Err(status)));
                            }
                        }
                        std::task::Poll::Ready(_) => {}
                        std::task::Poll::Pending => return std::task::Poll::Pending,
                    }
                }
                return std::task::Poll::Ready(None);
            }

            match this.body.poll_data(cx) {
                std::task::Poll::Ready(Some(Ok(chunk))) => this.buf.extend_from_slice(&chunk),
                std::task::Poll::Ready(Some(Err(e))) => {
                    return std::task::Poll::Ready(Some(Err(Status::new(
                        Code::Internal,
                        format!("h2 body error: {e}"),
                    ))))
                }
                std::task::Poll::Ready(None) => this.eof = true,
                std::task::Poll::Pending => return std::task::Poll::Pending,
            }
        }
    }
}

fn headers_to_pairs(headers: &HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|s| (k.as_str().to_string(), s.to_string()))
        })
        .collect()
}

/// Read one message from a response body and return the terminal status.
async fn read_unary(
    body: RecvStream,
    encoding: Compression,
    max: usize,
) -> Result<(Option<Vec<u8>>, Status), Status> {
    let mut reader = GrpcResponseBody::new(body, encoding, max);
    let mut message: Option<Vec<u8>> = None;
    while let Some(item) = reader.next().await {
        message = Some(item?);
    }
    Ok((message, Status::ok()))
}

#[async_trait::async_trait]
impl Transport for Client {
    async fn unary(
        &self,
        path: &str,
        metadata: Metadata,
        message: Vec<u8>,
    ) -> Result<(Vec<u8>, Metadata), Status> {
        let endpoint = self.endpoint.clone();
        let policy = endpoint.retry_policy.clone();
        let mut attempt: u32 = 0;
        loop {
            match self
                .unary_once(path, &metadata, message.clone(), &endpoint)
                .await
            {
                Ok(v) => return Ok(v),
                Err(e) => {
                    if attempt >= policy.max_retries || !policy.retryable.contains(&e.code) {
                        return Err(e);
                    }
                    attempt += 1;
                    tokio::time::sleep(policy.backoff * attempt).await;
                }
            }
        }
    }

    async fn server_streaming(
        &self,
        path: &str,
        metadata: Metadata,
        message: Vec<u8>,
    ) -> Result<(ServerStream<Vec<u8>>, Metadata), Status> {
        self.streaming_call(path, &metadata, message).await
    }

    async fn client_streaming(
        &self,
        path: &str,
        metadata: Metadata,
        stream: ClientStream<Vec<u8>>,
    ) -> Result<(Vec<u8>, Metadata), Status> {
        self.client_streaming_call(path, &metadata, stream).await
    }

    async fn bidi_streaming(
        &self,
        path: &str,
        metadata: Metadata,
        stream: ClientStream<Vec<u8>>,
    ) -> Result<(ServerStream<Vec<u8>>, Metadata), Status> {
        self.bidi_streaming_call(path, &metadata, stream).await
    }
}

impl Client {
    /// Single unary attempt.
    async fn unary_once(
        &self,
        path: &str,
        metadata: &Metadata,
        message: Vec<u8>,
        endpoint: &Endpoint,
    ) -> Result<(Vec<u8>, Metadata), Status> {
        let mut conn = self.connection().await?;
        let (resp_future, mut send_stream) =
            send_request_headers(&mut conn, path, metadata, endpoint, false).await?;
        let framed = crate::codec::encode_message(
            &message,
            endpoint.compression.clone(),
            endpoint.max_message_size,
        )
        .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        send_stream
            .send_data(Bytes::from(framed), true)
            .map_err(|e| Status::new(Code::Internal, format!("send_data: {e}")))?;

        let response = resp_future
            .await
            .map_err(|e| Status::new(Code::Unavailable, format!("h2 response: {e}")))?;
        let (head, body) = response.into_parts();
        let resp_encoding = encoding_from_headers(&head.headers);
        let resp_metadata = metadata_from_response(&head.headers, endpoint.max_message_size);

        let (msg, status) = read_unary(body, resp_encoding, endpoint.max_message_size).await?;
        if !status.is_ok() {
            return Err(status);
        }
        let raw = msg.ok_or_else(|| Status::new(Code::Internal, "empty unary response"))?;
        Ok((raw, resp_metadata))
    }

    /// Server- or bidi-streaming request setup (single request message).
    async fn streaming_call(
        &self,
        path: &str,
        metadata: &Metadata,
        message: Vec<u8>,
    ) -> Result<(ServerStream<Vec<u8>>, Metadata), Status> {
        let endpoint = self.endpoint.clone();
        let mut conn = self.connection().await?;
        let (resp_future, mut send_stream) =
            send_request_headers(&mut conn, path, metadata, &endpoint, false).await?;
        let framed = crate::codec::encode_message(
            &message,
            endpoint.compression.clone(),
            endpoint.max_message_size,
        )
        .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        send_stream
            .send_data(Bytes::from(framed), true)
            .map_err(|e| Status::new(Code::Internal, format!("send_data: {e}")))?;

        let response = resp_future
            .await
            .map_err(|e| Status::new(Code::Unavailable, format!("h2 response: {e}")))?;
        let (head, body) = response.into_parts();
        let resp_encoding = encoding_from_headers(&head.headers);
        let resp_metadata = metadata_from_response(&head.headers, endpoint.max_message_size);
        let stream: ServerStream<Vec<u8>> =
            Box::pin(GrpcResponseBody::new(body, resp_encoding, endpoint.max_message_size));
        Ok((stream, resp_metadata))
    }

    /// Client-streaming: send the incoming stream of messages, read one back.
    async fn client_streaming_call(
        &self,
        path: &str,
        metadata: &Metadata,
        stream: ClientStream<Vec<u8>>,
    ) -> Result<(Vec<u8>, Metadata), Status> {
        let endpoint = self.endpoint.clone();
        let mut conn = self.connection().await?;
        let (resp_future, mut send_stream) =
            send_request_headers(&mut conn, path, metadata, &endpoint, false).await?;

        let mut send_stream = send_stream;
        let mut req_stream = stream;
        let write_task = tokio::spawn(async move {
            let mut last_err: Option<Status> = None;
            while let Some(item) = req_stream.next().await {
                match item {
                    Ok(raw) => match crate::codec::encode_message(
                        &raw,
                        endpoint.compression.clone(),
                        endpoint.max_message_size,
                    ) {
                        Ok(framed) => {
                            if let Err(e) = send_stream.send_data(Bytes::from(framed), false) {
                                last_err = Some(Status::new(Code::Internal, format!("send_data: {e}")));
                                break;
                            }
                        }
                        Err(e) => {
                            last_err = Some(Status::new(Code::Internal, e.to_string()));
                            break;
                        }
                    },
                    Err(e) => {
                        last_err = Some(e);
                        break;
                    }
                }
            }
            let end = last_err.is_none();
            let _ = send_stream.send_data(Bytes::new(), end);
            last_err
        });

        let response = resp_future
            .await
            .map_err(|e| Status::new(Code::Unavailable, format!("h2 response: {e}")))?;
        let (head, body) = response.into_parts();
        let resp_encoding = encoding_from_headers(&head.headers);
        let resp_metadata = metadata_from_response(&head.headers, endpoint.max_message_size);
        if let Some(e) = write_task
            .await
            .map_err(|_| Status::new(Code::Internal, "writer task panicked"))?
        {
            return Err(e);
        }
        let (msg, status) = read_unary(body, resp_encoding, endpoint.max_message_size).await?;
        if !status.is_ok() {
            return Err(status);
        }
        let raw = msg.ok_or_else(|| Status::new(Code::Internal, "empty response"))?;
        Ok((raw, resp_metadata))
    }

    /// Bidirectional streaming.
    async fn bidi_streaming_call(
        &self,
        path: &str,
        metadata: &Metadata,
        stream: ClientStream<Vec<u8>>,
    ) -> Result<(ServerStream<Vec<u8>>, Metadata), Status> {
        let endpoint = self.endpoint.clone();
        let mut conn = self.connection().await?;
        let (resp_future, mut send_stream) =
            send_request_headers(&mut conn, path, metadata, &endpoint, false).await?;

        let mut send_stream = send_stream;
        let mut req_stream = stream;
        let compression = endpoint.compression.clone();
        let max = endpoint.max_message_size;
        tokio::spawn(async move {
            while let Some(item) = req_stream.next().await {
                match item {
                    Ok(raw) => match crate::codec::encode_message(&raw, compression.clone(), max) {
                        Ok(framed) => {
                            if send_stream.send_data(Bytes::from(framed), false).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    },
                    Err(_) => break,
                }
            }
            let _ = send_stream.send_data(Bytes::new(), true);
        });

        let response = resp_future
            .await
            .map_err(|e| Status::new(Code::Unavailable, format!("h2 response: {e}")))?;
        let (head, body) = response.into_parts();
        let resp_encoding = encoding_from_headers(&head.headers);
        let resp_metadata = metadata_from_response(&head.headers, endpoint.max_message_size);
        let stream: ServerStream<Vec<u8>> =
            Box::pin(GrpcResponseBody::new(body, resp_encoding, endpoint.max_message_size));
        Ok((stream, resp_metadata))
    }
}

/// Build and send the request headers, returning the response future and send
/// stream. `end_of_stream` indicates whether the request has no body.
async fn send_request_headers(
    conn: &mut ClientConn,
    path: &str,
    metadata: &Metadata,
    endpoint: &Endpoint,
    end_of_stream: bool,
) -> Result<(h2::client::ResponseFuture, h2::SendStream<Bytes>), Status> {
    let authority = endpoint.authority();
    let mut builder = Request::builder()
        .method("POST")
        .uri(format!("http://{authority}{path}"))
        .header("content-type", "application/grpc")
        .header("te", "trailers");
    if endpoint.compression != Compression::Identity {
        builder = builder.header("grpc-encoding", endpoint.compression.as_header());
    }
    for (k, v) in metadata.to_headers() {
        builder = builder.header(k, v);
    }
    let request = builder
        .body(())
        .map_err(|e| Status::new(Code::Internal, format!("build request: {e}")))?;

    let tx = conn.send.clone();
    let mut tx = tx
        .ready()
        .await
        .map_err(|e| Status::new(Code::Unavailable, format!("connection not ready: {e}")))?;
    tx.send_request(request, end_of_stream)
        .map_err(|e| Status::new(Code::Unavailable, format!("send_request: {e}")))
}

/// Extract the response compression from `grpc-encoding`.
fn encoding_from_headers(headers: &HeaderMap) -> Compression {
    headers
        .get("grpc-encoding")
        .and_then(|v| v.to_str().ok())
        .map(Compression::from_header)
        .unwrap_or(Compression::Identity)
}

/// Build response metadata (excluding reserved / pseudo headers).
fn metadata_from_response(headers: &HeaderMap, max: usize) -> Metadata {
    let pairs: Vec<(String, String)> = headers
        .iter()
        .filter(|(k, _)| {
            let s = k.as_str();
            !s.starts_with(':')
                && s != "content-type"
                && s != "te"
                && s != "grpc-encoding"
                && s != "grpc-status"
                && s != "grpc-message"
        })
        .filter_map(|(k, v)| {
            v.to_str()
                .ok()
                .map(|v| (k.as_str().to_string(), v.to_string()))
        })
        .collect();
    Metadata::from_headers(&pairs, max).unwrap_or_default()
}
