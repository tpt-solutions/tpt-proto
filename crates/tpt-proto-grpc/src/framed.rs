//! Conversion between h2 request/response bodies and gRPC message streams.
//!
//! On the wire each gRPC message is `[1-byte flag][4-byte big-endian length]
//! [payload]`. A single HTTP/2 DATA stream may carry zero or more such
//! messages, and a single message may be split across several DATA frames. These
//! helpers buffer incoming bytes, split them into individual messages (applying
//! per-message decompression), and frame outgoing messages for the send side.

use std::pin::Pin;
use std::task::{Context, Poll};

use bytes::{Bytes, BytesMut};
use futures::{Stream, StreamExt};
use h2::RecvStream;

use crate::codec::decode_message;
use crate::compression::Compression;
use crate::status::{Code, Status};
use crate::transport::{ClientStream, ServerStream};

/// Wrap an h2 [`RecvStream`] as a `futures::Stream` of raw deframed gRPC
/// messages, decompressing each with `encoding`.
///
/// Errors decoding a frame are surfaced as a terminal `Err` item; the stream
/// then ends.
pub fn deframe_stream(
    body: RecvStream,
    encoding: Compression,
    max_message_size: usize,
) -> impl Stream<Item = Result<Vec<u8>, Status>> {
    DeframeStream {
        body,
        buf: BytesMut::new(),
        encoding,
        max: max_message_size,
        eof: false,
    }
}

struct DeframeStream {
    body: RecvStream,
    buf: BytesMut,
    encoding: Compression,
    max: usize,
    eof: bool,
}

impl Stream for DeframeStream {
    type Item = Result<Vec<u8>, Status>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        loop {
            // Try to extract a complete frame from the buffer.
            if this.buf.len() >= 5 {
                let declared = u32::from_be_bytes([this.buf[1], this.buf[2], this.buf[3], this.buf[4]])
                    as usize;
                if declared > this.max {
                    return Poll::Ready(Some(Err(Status::new(
                        Code::ResourceExhausted,
                        format!(
                            "declared message length {declared} exceeds maximum {}",
                            this.max
                        ),
                    ))));
                }
                let total = 5 + declared;
                if this.buf.len() >= total {
                    let frame: Bytes = this.buf.split_to(total).freeze();
                    let raw = match decode_message(&frame, this.encoding.clone(), this.max) {
                        Ok(r) => r,
                        Err(e) => {
                            return Poll::Ready(Some(Err(Status::new(
                                Code::Internal,
                                format!("failed to decode gRPC frame: {e}"),
                            ))))
                        }
                    };
                    return Poll::Ready(Some(Ok(raw)));
                }
            }

            if this.eof {
                if this.buf.is_empty() {
                    return Poll::Ready(None);
                }
                return Poll::Ready(Some(Err(Status::new(
                    Code::Internal,
                    "connection closed with a partial gRPC frame",
                ))));
            }

            match this.body.poll_data(cx) {
                Poll::Ready(Some(Ok(chunk))) => {
                    this.buf.extend_from_slice(&chunk);
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(Status::new(
                        Code::Internal,
                        format!("h2 body error: {e}"),
                    ))));
                }
                Poll::Ready(None) => {
                    this.eof = true;
                    // Loop again to flush any complete buffered frame, then end.
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

/// Read the full set of deframed messages from a body, concatenating nothing but
/// returning the last (unary) message. Returns `None` if the stream was empty.
pub async fn read_single_message(
    body: RecvStream,
    encoding: Compression,
    max: usize,
) -> Result<Option<Vec<u8>>, Status> {
    let mut stream = deframe_stream(body, encoding, max);
    let mut last: Option<Vec<u8>> = None;
    while let Some(item) = stream.next().await {
        last = Some(item?);
    }
    Ok(last)
}

/// Map a `ServerStream<A>` into a `ServerStream<B>` using a fallible mapper.
///
/// Each `Ok(a)` item is transformed via `f`; `Err` items pass through
/// unchanged.
pub fn map_server_stream<A, B, F>(
    stream: ServerStream<A>,
    f: F,
) -> ServerStream<B>
where
    A: Send + 'static,
    B: Send + 'static,
    F: Fn(A) -> Result<B, Status> + Send + 'static,
{
    Box::pin(stream.map(move |r| r.and_then(|a| f(a))))
}

/// Map a `ClientStream<A>` (the request side) into a `ClientStream<B>`.
pub fn map_client_stream<A, B, F>(
    stream: ClientStream<A>,
    f: F,
) -> ClientStream<B>
where
    A: Send + 'static,
    B: Send + 'static,
    F: Fn(A) -> Result<B, Status> + Send + 'static,
{
    Box::pin(stream.map(move |r| r.and_then(|a| f(a))))
}
