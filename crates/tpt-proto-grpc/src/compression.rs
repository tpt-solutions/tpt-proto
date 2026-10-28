//! Compression algorithms and codecs used by the gRPC message framing layer.
//!
//! gRPC permits `identity` (no compression) and `gzip` out of the box, plus
//! additional pluggable codecs identified by their `grpc-encoding` header
//! value.

use anyhow::{bail, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression as GzLevel;
use std::io::{Read, Write};

/// A gRPC compression algorithm, identified by its `grpc-encoding` header value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Compression {
    /// No compression.
    Identity,
    /// gzip compression.
    Gzip,
    /// A pluggable, unsupported-at-runtime codec identified by its header name.
    Other(String),
}

impl Compression {
    /// Parse a `grpc-encoding` header value into a [`Compression`].
    ///
    /// An empty string is treated as `identity`.
    pub fn from_header(value: &str) -> Compression {
        match value {
            "" | "identity" => Compression::Identity,
            "gzip" => Compression::Gzip,
            other => Compression::Other(other.to_ascii_lowercase()),
        }
    }

    /// Render this algorithm as its `grpc-encoding` header value.
    pub fn as_header(&self) -> &str {
        match self {
            Compression::Identity => "identity",
            Compression::Gzip => "gzip",
            Compression::Other(name) => name.as_str(),
        }
    }

    /// Alias of [`Compression::as_header`].
    pub fn name(&self) -> &str {
        self.as_header()
    }

    /// Compress `data`, returning the compressed payload.
    pub fn compress(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self {
            Compression::Identity => Ok(data.to_vec()),
            Compression::Gzip => {
                let mut enc = GzEncoder::new(Vec::new(), GzLevel::default());
                enc.write_all(data)?;
                Ok(enc.finish()?)
            }
            Compression::Other(_) => bail!("unsupported compression algorithm"),
        }
    }

    /// Decompress `data`, returning the original payload.
    ///
    /// `max_size` bounds the decompressed output: decompression is aborted as
    /// soon as more than `max_size` bytes have been produced, so a small
    /// compressed input crafted to expand to an enormous payload (a
    /// "decompression bomb") cannot exhaust memory before the size limit is
    /// enforced.
    pub fn decompress(&self, data: &[u8], max_size: usize) -> Result<Vec<u8>> {
        match self {
            Compression::Identity => Ok(data.to_vec()),
            Compression::Gzip => {
                let dec = GzDecoder::new(data);
                // Read at most one byte beyond the cap so we can distinguish
                // "decompressed to exactly max_size" from "there was more
                // data beyond the limit" in a single pass, without ever
                // materializing an oversized buffer.
                let cap = (max_size as u64).saturating_add(1);
                let mut limited = dec.take(cap);
                let mut out = Vec::new();
                limited.read_to_end(&mut out)?;
                if out.len() as u64 > max_size as u64 {
                    bail!("decompressed message length exceeds maximum {}", max_size);
                }
                Ok(out)
            }
            Compression::Other(_) => bail!("unsupported compression algorithm"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_roundtrip() {
        assert_eq!(Compression::from_header("gzip"), Compression::Gzip);
        assert_eq!(Compression::from_header(""), Compression::Identity);
        assert_eq!(Compression::from_header("identity"), Compression::Identity);
        assert_eq!(
            Compression::from_header("snappy"),
            Compression::Other("snappy".into())
        );
        assert_eq!(Compression::Gzip.as_header(), "gzip");
    }

    #[test]
    fn identity_is_passthrough() {
        let data = b"hello world";
        assert_eq!(Compression::Identity.compress(data).unwrap(), data);
        assert_eq!(
            Compression::Identity.decompress(data, usize::MAX).unwrap(),
            data
        );
    }

    #[test]
    fn gzip_roundtrip() {
        let data: Vec<u8> = (0..1024).map(|i| (i % 251) as u8).collect();
        let compressed = Compression::Gzip.compress(&data).unwrap();
        assert!(compressed.len() < data.len());
        assert_eq!(
            Compression::Gzip
                .decompress(&compressed, usize::MAX)
                .unwrap(),
            data
        );
    }

    #[test]
    fn unsupported_fails() {
        let c = Compression::Other("snappy".into());
        assert!(c.compress(b"x").is_err());
        assert!(c.decompress(b"x", usize::MAX).is_err());
    }

    #[test]
    fn gzip_decompression_bomb_is_rejected() {
        // Highly compressible: a large run of zeroes compresses to a small
        // fraction of its expanded size (DEFLATE encodes long back-references
        // in a few bits each, topping out around ~1000:1 on zero runs), yet
        // expands back to tens of megabytes — far past the small cap below.
        let data = vec![0u8; 50 * 1024 * 1024];
        let compressed = Compression::Gzip.compress(&data).unwrap();
        assert!(
            compressed.len() < data.len() / 256,
            "expected a compact compressed blob, got {}",
            compressed.len()
        );

        let small_cap = 1024;
        let err = Compression::Gzip
            .decompress(&compressed, small_cap)
            .unwrap_err();
        assert!(err.to_string().contains("exceeds maximum"));
    }

    #[test]
    fn gzip_decompress_exact_cap_succeeds() {
        let data: Vec<u8> = (0..4096).map(|i| (i % 200) as u8).collect();
        let compressed = Compression::Gzip.compress(&data).unwrap();
        let out = Compression::Gzip
            .decompress(&compressed, data.len())
            .unwrap();
        assert_eq!(out, data);
    }
}
