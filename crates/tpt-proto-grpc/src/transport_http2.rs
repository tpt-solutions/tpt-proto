//! HTTP/2 gRPC client transport (gRPC over HTTP/2, cleartext `h2c`).
//!
//! Implements the [`Transport`](crate::transport::Transport) trait using the
//! `h2` crate, allowing generated client stubs and the `tpt-grpc` debug CLI to
//! issue real unary and streaming calls. Each call opens a fresh prior-knowledge
//! HTTP/2 connection; the connection driver is spawned on the tokio runtime.
//!
//! TLS (and mTLS) client support is provided by the transport's TLS integration
//! in Phase 14; this implementation targets cleartext HTTP/2, which the gRPC
//! spec permits for local development.

use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream;
use futures::StreamExt;
use http::header::{HeaderMap, HeaderName, HeaderValue};
use http::Request as HttpRequest;
use tokio::net::TcpStream;

use crate::codec::{decode_message, encode_message, MAX_MESSAGE_SIZE_DEFAULT};
use crate::compression::Compression;
use crate::metadata::Metadata;
use crate::status::{Code, Status};
use crate::transport::{ClientStream, ServerStream, Transport};

/// A cleartext HTTP/2 gRPC client transport.
#[derive(Clone, Debug)]
pub struct H2Transport {
    /// The target, e.g. `localhost:50051` or `http://localhost:50051`.
    endpoint: String,
    /// The compression applied to outgoing messages.
    compression: Compression,
    /// Maximum incoming/outgoing message size.
    max_message_size: usize,
    /// Optional request timeout.
    timeout: Option<Duration>,
}

impl H2Transport {
    /// Construct a transport targeting `endpoint`.
    pub fn new(endpoint: impl Into<String>) -> Self {
        H2Transport {
            endpoint: endpoint.into(),
            compression: Compression::Identity,
            max_message_size: MAX_MESSAGE_SIZE_DEFAULT,
            timeout: None,
        }
    }

    /// Set the outgoing compression.
    pub fn with_compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }

    /// Set the maximum message size.
    pub fn with_max_message_size(mut self, max: usize) -> Self {
        self.max_message_size = max;
        self
    }

    /// Set a per-call timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn host_and_port(&self) -> Result<(String, u16), Status> {
        let s = self.endpoint.trim();
        let s = s
            .strip_prefix("http://")
            .or_else(|| s.strip_prefix("https://"))
            .unwrap_or(s);
        let (host, port) = s.rsplit_once(':').ok_or_else(|| {
            Status::new(Code::Internal, format!("invalid endpoint '{}'", self.endpoint))
        })?;
        let port = port
            .parse::<u16>()
            .map_err(|_| Status::new(Code::Internal, format!("invalid port in '{}'", self.endpoint)))?;
        Ok((host.to_string(), port))
    }

    fn build_request(&self, path: &str, metadata: &Metadata) -> Result<HttpRequest<()>, Status> {
        let (host, port) = self.host_and_port()?;
        let mut builder = HttpRequest::builder()
            .method("POST")
            .uri(format!("http://{host}:{port}{path}"))
            .header("content-type", crate::CONTENT_TYPE_GRPC)
            .header("te", "trailers")
            .header("user-agent", "tpt-proto-grpc/0.1");
        for (k, v) in metadata.to_headers() {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_bytes(k.as_bytes()),
                HeaderValue::from_bytes(v.as_bytes()),
            ) {
                builder = builder.header(name, val);
            }
        }
        if self.compression != Compression::Identity {
            builder = builder.header("grpc-encoding", self.compression.as_header());
        }
        if let Some(t) = self.timeout {
            builder = builder.header("grpc-timeout", crate::timeout::format_timeout(t));
        }
        builder
            .body(())
            .map_err(|e| Status::new(Code::Internal, format!("build request: {e}")))
    }

    /// Open a connection, send the request headers and (optional) body, then
    /// collect the response body bytes, response headers, and trailers.
    async fn exchange(
        &self,
        path: &str,
        metadata: Metadata,
        request_body: Option<Vec<u8>>,
        mut client_stream: Option<ClientStream<Vec<u8>>>,
    ) -> Result<(Vec<u8>, HeaderMap, Option<HeaderMap>), Status> {
        let (host, port) = self.host_and_port()?;
        let tcp = TcpStream::connect((host.as_str(), port))
            .await
            .map_err(|e| Status::new(Code::Unavailable, format!("connect: {e}")))?;
        tcp.set_nodelay(true).ok();
        let (mut send, conn) = h2::client::handshake(tcp)
            .await
            .map_err(|e| Status::new(Code::Unavailable, format!("h2 handshake: {e}")))?;
        // Drive the connection to completion in the background.
        tokio::spawn(async move {
            let _ = conn.await;
        });

        let req = self.build_request(path, &metadata)?;
        let (resp_future, mut body_send) = send
            .send_request(req, false)
            .map_err(|e| Status::new(Code::Unavailable, format!("send request: {e}")))?;
        let resp = resp_future
            .await
            .map_err(|e| Status::new(Code::Unavailable, format!("response: {e}")))?;

        // Send the request body.
        if let Some(bytes) = request_body {
            body_send
                .send_data(Bytes::from(bytes), true)
                .map_err(|e| Status::new(Code::Internal, format!("send data: {e}")))?;
        } else if let Some(mut cs) = client_stream.take() {
            while let Some(msg) = cs.next().await {
                let msg =
                    msg.map_err(|s| Status::new(Code::Internal, format!("client stream error: {s}")))?;
                let framed = encode_message(&msg, self.compression.clone(), self.max_message_size)
                    .map_err(|e| Status::new(Code::Internal, format!("frame: {e}")))?;
                body_send
                    .send_data(Bytes::from(framed), false)
                    .map_err(|e| Status::new(Code::Internal, format!("send data: {e}")))?;
            }
            body_send
                .send_data(Bytes::new(), true)
                .map_err(|e| Status::new(Code::Internal, format!("end stream: {e}")))?;
        } else {
            body_send
                .send_data(Bytes::new(), true)
                .map_err(|e| Status::new(Code::Internal, format!("end stream: {e}")))?;
        }

        let (parts, mut recv) = resp.into_parts();
        let body = collect_body(&mut recv, self.max_message_size).await?;
        let trailers = recv
            .trailers()
            .await
            .map_err(|e| Status::new(Code::Internal, format!("trailers: {e}")))?;
        Ok((body, parts.headers, trailers))
    }
}

/// Collect the full response DATA into a single buffer (we parse gRPC framing
/// from the buffer afterwards).
async fn collect_body(recv: &mut h2::RecvStream, max: usize) -> Result<Vec<u8>, Status> {
    let mut buf = Vec::new();
    while let Some(chunk) = recv.data().await {
        let chunk = chunk.map_err(|e| Status::new(Code::Internal, format!("recv data: {e}")))?;
        buf.extend_from_slice(&chunk);
        if buf.len() > max {
            return Err(Status::new(
                Code::ResourceExhausted,
                format!("response body exceeds {max} bytes"),
            ));
        }
    }
    Ok(buf)
}

/// Parse concatenated gRPC-framed messages from `buf`, yielding the raw framed
/// bytes (compression flag + 4-byte length + payload) for each message.
fn split_grpc_frames(buf: &[u8]) -> Result<Vec<Vec<u8>>, Status> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < buf.len() {
        if i + 5 > buf.len() {
            return Err(Status::new(
                Code::Internal,
                "truncated gRPC frame header",
            ));
        }
        let len = u32::from_be_bytes([buf[i + 1], buf[i + 2], buf[i + 3], buf[i + 4]]) as usize;
        let total = 5 + len;
        if total > buf.len() {
            return Err(Status::new(Code::Internal, "truncated gRPC message body"));
        }
        out.push(buf[i..total].to_vec());
        i = total;
    }
    Ok(out)
}

#[async_trait]
impl Transport for H2Transport {
    async fn unary(
        &self,
        path: &str,
        metadata: Metadata,
        message: Vec<u8>,
    ) -> Result<(Vec<u8>, Metadata), Status> {
        let framed = encode_message(&message, self.compression.clone(), self.max_message_size)
            .map_err(|e| Status::new(Code::Internal, format!("frame: {e}")))?;
        let (body, headers, trailers) = self.exchange(path, metadata, Some(framed), None).await?;
        let frames = split_grpc_frames(&body)?;
        let raw = frames
            .into_iter()
            .next()
            .ok_or_else(|| Status::new(Code::Internal, "empty response body"))?;
        let payload = decode_message(&raw, self.compression.clone(), self.max_message_size)
            .map_err(|e| Status::new(Code::Internal, format!("unframe: {e}")))?;
        let resp_meta = merge_metadata(&headers, trailers.as_ref());
        Ok((payload, resp_meta))
    }

    async fn server_streaming(
        &self,
        path: &str,
        metadata: Metadata,
        message: Vec<u8>,
    ) -> Result<(ServerStream<Vec<u8>>, Metadata), Status> {
        let framed = encode_message(&message, self.compression.clone(), self.max_message_size)
            .map_err(|e| Status::new(Code::Internal, format!("frame: {e}")))?;
        let (body, headers, trailers) = self.exchange(path, metadata, Some(framed), None).await?;
        let resp_meta = merge_metadata(&headers, trailers.as_ref());
        let compression = self.compression.clone();
        let max = self.max_message_size;
        let frames = split_grpc_frames(&body)?;
        let stream = stream::iter(frames.into_iter().map(move |raw| {
            decode_message(&raw, compression.clone(), max)
                .map_err(|e| Status::new(Code::Internal, format!("unframe: {e}")))
        }));
        Ok((Box::pin(stream), resp_meta))
    }

    async fn client_streaming(
        &self,
        path: &str,
        metadata: Metadata,
        stream: ClientStream<Vec<u8>>,
    ) -> Result<(Vec<u8>, Metadata), Status> {
        let (body, headers, trailers) = self.exchange(path, metadata, None, Some(stream)).await?;
        let frames = split_grpc_frames(&body)?;
        let raw = frames
            .into_iter()
            .next()
            .ok_or_else(|| Status::new(Code::Internal, "empty response body"))?;
        let payload = decode_message(&raw, self.compression.clone(), self.max_message_size)
            .map_err(|e| Status::new(Code::Internal, format!("unframe: {e}")))?;
        let resp_meta = merge_metadata(&headers, trailers.as_ref());
        Ok((payload, resp_meta))
    }

    async fn bidi_streaming(
        &self,
        path: &str,
        metadata: Metadata,
        stream: ClientStream<Vec<u8>>,
    ) -> Result<(ServerStream<Vec<u8>>, Metadata), Status> {
        let (body, headers, trailers) = self.exchange(path, metadata, None, Some(stream)).await?;
        let resp_meta = merge_metadata(&headers, trailers.as_ref());
        let compression = self.compression.clone();
        let max = self.max_message_size;
        let frames = split_grpc_frames(&body)?;
        let stream = stream::iter(frames.into_iter().map(move |raw| {
            decode_message(&raw, compression.clone(), max)
                .map_err(|e| Status::new(Code::Internal, format!("unframe: {e}")))
        }));
        Ok((Box::pin(stream), resp_meta))
    }
}

/// Convert response HTTP headers and trailers into gRPC [`Metadata`].
fn merge_metadata(headers: &HeaderMap, trailers: Option<&HeaderMap>) -> Metadata {
    let mut md = Metadata::new();
    let mut add = |h: &HeaderMap| {
        for (name, value) in h.iter() {
            let name = name.as_str();
            if name.starts_with(':') {
                continue;
            }
            if let Ok(v) = value.to_str() {
                let _ = md.insert_text(name, v);
            }
        }
    };
    add(headers);
    if let Some(t) = trailers {
        add(t);
    }
    md
}
