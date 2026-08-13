//! gRPC message framing: `[1-byte compression flag][4-byte big-endian length][payload]`.
//!
//! Each gRPC message on the wire is framed exactly this way. The compression
//! flag is `0` for uncompressed payloads and `1` when the payload has been
//! compressed with the algorithm negotiated via the `grpc-encoding` header.
//! Malformed frames (too short, length mismatch, unknown flag, or exceeding
//! the configured maximum) are rejected so callers can reset the stream.

use anyhow::{bail, Result};

use crate::compression::Compression;

/// Compression flag value indicating an uncompressed payload.
pub const COMPRESSION_FLAG_UNCOMPRESSED: u8 = 0;
/// Compression flag value indicating a compressed payload.
pub const COMPRESSION_FLAG_COMPRESSED: u8 = 1;

/// Default maximum permitted message size: 4 MiB.
pub const MAX_MESSAGE_SIZE_DEFAULT: usize = 4 * 1024 * 1024;

/// Encode a single gRPC message frame.
///
/// When `compression` is not [`Compression::Identity`], the payload is
/// compressed and the compression flag is set to
/// [`COMPRESSION_FLAG_COMPRESSED`].
pub fn encode_message(
    message: &[u8],
    compression: Compression,
    max_message_size: usize,
) -> Result<Vec<u8>> {
    if message.len() > max_message_size {
        bail!(
            "message length {} exceeds maximum {}",
            message.len(),
            max_message_size
        );
    }
    let compressed = compression.compress(message)?;
    let (flag, payload) = if compression == Compression::Identity {
        (COMPRESSION_FLAG_UNCOMPRESSED, message.to_vec())
    } else {
        (COMPRESSION_FLAG_COMPRESSED, compressed)
    };
    let mut out = Vec::with_capacity(payload.len() + 5);
    out.push(flag);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(&payload);
    Ok(out)
}

/// Decode a single gRPC message frame.
///
/// `encoding` is the algorithm the sender used (signalled via the
/// `grpc-encoding` header). It is only applied when the frame's compression
/// flag is set.
pub fn decode_message(
    frame: &[u8],
    encoding: Compression,
    max_message_size: usize,
) -> Result<Vec<u8>> {
    if frame.len() < 5 {
        bail!("frame too short: {} bytes (minimum 5)", frame.len());
    }
    let flag = frame[0];
    let declared = u32::from_be_bytes([frame[1], frame[2], frame[3], frame[4]]) as usize;
    if declared > max_message_size {
        bail!(
            "declared message length {} exceeds maximum {}",
            declared,
            max_message_size
        );
    }
    let payload = &frame[5..];
    if payload.len() != declared {
        bail!(
            "frame payload length {} does not match declared length {}",
            payload.len(),
            declared
        );
    }
    match flag {
        COMPRESSION_FLAG_UNCOMPRESSED => Ok(payload.to_vec()),
        COMPRESSION_FLAG_COMPRESSED => {
            let data = encoding.decompress(payload)?;
            if data.len() > max_message_size {
                bail!(
                    "decompressed message length {} exceeds maximum {}",
                    data.len(),
                    max_message_size
                );
            }
            Ok(data)
        }
        other => bail!("invalid compression flag {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncompressed_roundtrip() {
        let msg = b"the quick brown fox";
        let frame = encode_message(msg, Compression::Identity, usize::MAX).unwrap();
        assert_eq!(frame[0], COMPRESSION_FLAG_UNCOMPRESSED);
        assert_eq!(&frame[1..5], &(msg.len() as u32).to_be_bytes());
        let back = decode_message(&frame, Compression::Identity, usize::MAX).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn compressed_roundtrip() {
        let msg: Vec<u8> = (0..2048).map(|i| (i % 200) as u8).collect();
        let frame = encode_message(msg.as_slice(), Compression::Gzip, usize::MAX).unwrap();
        assert_eq!(frame[0], COMPRESSION_FLAG_COMPRESSED);
        let back = decode_message(&frame, Compression::Gzip, usize::MAX).unwrap();
        assert_eq!(back, msg);
    }

    #[test]
    fn rejects_short_frame() {
        assert!(decode_message(b"\x00\x00", Compression::Identity, usize::MAX).is_err());
    }

    #[test]
    fn rejects_length_mismatch() {
        let mut frame = vec![0u8; 9];
        frame[1..5].copy_from_slice(&8u32.to_be_bytes());
        frame.extend_from_slice(b"tooshort");
        assert!(decode_message(&frame, Compression::Identity, usize::MAX).is_err());
    }

    #[test]
    fn rejects_bad_flag() {
        let mut frame = vec![9u8; 5];
        frame.extend_from_slice(b"hello");
        assert!(decode_message(&frame, Compression::Identity, usize::MAX).is_err());
    }

    #[test]
    fn enforces_max_size() {
        let msg = b"hello";
        assert!(encode_message(msg, Compression::Identity, 3).is_err());
        let frame = encode_message(msg, Compression::Identity, 16).unwrap();
        assert!(decode_message(&frame, Compression::Identity, 3).is_err());
    }
}
