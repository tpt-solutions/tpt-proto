//! Transport abstraction and channel for gRPC calls.
//!
//! The [`Transport`] trait is the single seam between generated client stubs
//! and the underlying HTTP/2 implementation. A minimal in-memory transport is
//! provided for testing; production use plugs in an HTTP/2 transport that
//! frames messages with [`crate::codec`] and drives the four call patterns.

use std::sync::Arc;

use async_trait::async_trait;
use futures::stream::BoxStream;

use tpt_proto_core::Message;

use crate::codec::{decode_message, encode_message, MAX_MESSAGE_SIZE_DEFAULT};
use crate::compression::Compression;
use crate::context::Request;
use crate::metadata::Metadata;
use crate::status::{Code, Status};

/// A server-streaming or bidi-streaming response payload: a stream of
/// messages (or a terminal [`Status`] error).
pub type ServerStream<M> = BoxStream<'static, Result<M, Status>>;

/// A client-streaming or bidi-streaming request payload: a stream of messages
/// (or a terminal [`Status`] error) produced by the caller.
pub type ClientStream<M> = BoxStream<'static, Result<M, Status>>;

/// The transport seam used by generated client stubs.
///
/// Implementations frame each message with [`crate::codec`] and translate
/// gRPC status/trailers. All four call patterns are represented; a
/// transport that does not support a pattern may return
/// [`Code::Unimplemented`].
#[async_trait]
pub trait Transport: Send + Sync {
    /// Unary call: one request, one response.
    async fn unary(
        &self,
        path: &str,
        metadata: Metadata,
        message: Vec<u8>,
    ) -> Result<(Vec<u8>, Metadata), Status>;

    /// Server-streaming call: one request, many responses.
    async fn server_streaming(
        &self,
        path: &str,
        metadata: Metadata,
        message: Vec<u8>,
    ) -> Result<(ServerStream<Vec<u8>>, Metadata), Status>;

    /// Client-streaming call: many requests, one response.
    async fn client_streaming(
        &self,
        path: &str,
        metadata: Metadata,
        stream: ClientStream<Vec<u8>>,
    ) -> Result<(Vec<u8>, Metadata), Status>;

    /// Bidirectional-streaming call: many requests, many responses.
    async fn bidi_streaming(
        &self,
        path: &str,
        metadata: Metadata,
        stream: ClientStream<Vec<u8>>,
    ) -> Result<(ServerStream<Vec<u8>>, Metadata), Status>;
}

/// A gRPC channel wrapping a [`Transport`].
///
/// Cloning a channel shares the underlying transport. The default compression
/// is `identity`; set [`Channel::with_compression`] to negotiate a different
/// `grpc-encoding` with the peer.
#[derive(Clone)]
pub struct Channel {
    transport: Arc<dyn Transport>,
    compression: Compression,
    max_message_size: usize,
}

impl Channel {
    /// Create a channel from a transport.
    pub fn new(transport: Arc<dyn Transport>) -> Self {
        Channel {
            transport,
            compression: Compression::Identity,
            max_message_size: MAX_MESSAGE_SIZE_DEFAULT,
        }
    }

    /// Set the compression algorithm applied to outgoing messages.
    pub fn with_compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }

    /// Set the maximum incoming/outgoing message size.
    pub fn with_max_message_size(mut self, max: usize) -> Self {
        self.max_message_size = max;
        self
    }

    /// The negotiated compression algorithm.
    pub fn compression(&self) -> Compression {
        self.compression.clone()
    }

    /// Perform a unary call, encoding `req` and decoding the response into `R`.
    pub async fn unary<M: Message + Default, R: Message + Default>(
        &self,
        path: &str,
        metadata: Metadata,
        req: &M,
    ) -> Result<(R, Metadata), Status> {
        let bytes = req
            .encode_to_vec()
            .map_err(|e| Status::new(Code::Internal, format!("encode: {e}")))?;
        let (out, trailers) = self.transport.unary(path, metadata, bytes).await?;
        let resp = R::decode(&out)
            .map_err(|e| Status::new(Code::Internal, format!("decode: {e}")))?;
        Ok((resp, trailers))
    }

    /// Encode a request message into a framed gRPC message using the channel's
    /// configured compression. Used by streaming client stubs.
    pub fn encode_message(&self, message: &impl Message) -> Result<Vec<u8>, Status> {
        let raw = message
            .encode_to_vec()
            .map_err(|e| Status::new(Code::Internal, format!("encode: {e}")))?;
        encode_message(&raw, self.compression.clone(), self.max_message_size)
            .map_err(|e| Status::new(Code::Internal, format!("frame: {e}")))
    }

    /// Decode a framed gRPC message body into `M` using the channel's
    /// configured compression. Used by streaming client stubs.
    pub fn decode_message<M: Message + Default>(&self, framed: &[u8]) -> Result<M, Status> {
        let raw = decode_message(framed, self.compression.clone(), self.max_message_size)
            .map_err(|e| Status::new(Code::Internal, format!("unframe: {e}")))?;
        M::decode(&raw).map_err(|e| Status::new(Code::Internal, format!("decode: {e}")))
    }

    /// Access the underlying transport (for streaming calls by the runtime).
    pub fn transport(&self) -> &dyn Transport {
        &*self.transport
    }
}

/// Helper to build a [`Request`] wrapper for client-stub calls.
pub fn request<M: Message>(message: M) -> Request<M> {
    Request::new(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream;
    use futures::StreamExt;
    use tpt_proto_core::Reader;

    struct EchoTransport;

    #[async_trait]
    impl Transport for EchoTransport {
        async fn unary(
            &self,
            _path: &str,
            metadata: Metadata,
            message: Vec<u8>,
        ) -> Result<(Vec<u8>, Metadata), Status> {
            Ok((message, metadata))
        }
        async fn server_streaming(
            &self,
            _path: &str,
            _metadata: Metadata,
            message: Vec<u8>,
        ) -> Result<(ServerStream<Vec<u8>>, Metadata), Status> {
            let s: ServerStream<Vec<u8>> = Box::pin(stream::once(async move { Ok(message) }));
            Ok((s, Metadata::new()))
        }
        async fn client_streaming(
            &self,
            _path: &str,
            _metadata: Metadata,
            _stream: ClientStream<Vec<u8>>,
        ) -> Result<(Vec<u8>, Metadata), Status> {
            Ok((Vec::new(), Metadata::new()))
        }
        async fn bidi_streaming(
            &self,
            _path: &str,
            _metadata: Metadata,
            stream: ClientStream<Vec<u8>>,
        ) -> Result<(ServerStream<Vec<u8>>, Metadata), Status> {
            let s: ServerStream<Vec<u8>> = Box::pin(stream.map(|r| r));
            Ok((s, Metadata::new()))
        }
    }

    #[derive(Debug, PartialEq)]
    struct Msg {
        body: Vec<u8>,
    }
    impl Default for Msg {
        fn default() -> Self {
            Msg { body: Vec::new() }
        }
    }
    impl Message for Msg {
        fn encode(&self, w: &mut tpt_proto_core::Writer) -> tpt_proto_core::Result<()> {
            tpt_proto_core::scalar::encode_bytes(w, 1, &self.body);
            Ok(())
        }
        fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
            let tag = r.read_tag()?;
            if tag.field_number == 1 {
                self.body = r.read_length_delimited()?.to_vec();
            } else {
                r.skip(tag.wire_type)?;
            }
            Ok(())
        }
    }

    #[test]
    fn channel_unary_roundtrip() {
        let ch = Channel::new(Arc::new(EchoTransport));
        let req = Msg {
            body: vec![1, 2, 3],
        };
        let fut = ch.unary::<Msg, Msg>("/pkg.Svc/Method", Metadata::new(), &req);
        let (resp, _) = futures::executor::block_on(fut).unwrap();
        assert_eq!(resp, Msg { body: vec![1, 2, 3] });
    }

    #[test]
    fn channel_clone_shares_transport() {
        let ch = Channel::new(Arc::new(EchoTransport)).with_compression(Compression::Gzip);
        let c2 = ch.clone();
        assert_eq!(c2.compression(), Compression::Gzip);
    }
}
