//! Compression algorithms and codecs used by the gRPC message framing layer.
//!
//! gRPC permits `identity` (no compression) and `gzip` out of the box, plus
//! additional pluggable codecs identified by their `grpc-encoding` header
//! value.

use anyhow::{bail, Result};
use flate2::write::GzEncoder;
use flate2::Compression as GzLevel;
use flate2::read::GzDecoder;
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
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>> {
        match self {
            Compression::Identity => Ok(data.to_vec()),
            Compression::Gzip => {
                let mut dec = GzDecoder::new(data);
                let mut out = Vec::new();
                dec.read_to_end(&mut out)?;
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
        assert_eq!(Compression::Identity.decompress(data).unwrap(), data);
    }

    #[test]
    fn gzip_roundtrip() {
        let data: Vec<u8> = (0..1024).map(|i| (i % 251) as u8).collect();
        let compressed = Compression::Gzip.compress(&data).unwrap();
        assert!(compressed.len() < data.len());
        assert_eq!(Compression::Gzip.decompress(&compressed).unwrap(), data);
    }

    #[test]
    fn unsupported_fails() {
        let c = Compression::Other("snappy".into());
        assert!(c.compress(b"x").is_err());
        assert!(c.decompress(b"x").is_err());
    }
}
