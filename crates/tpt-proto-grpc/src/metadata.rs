//! gRPC metadata: request/response headers and trailers.
//!
//! Metadata are key-value pairs. Keys are restricted to lowercase ASCII
//! (digits, letters, `-`, `_`, `.`). Binary values use the `-bin` suffix
//! convention and are base64-encoded on the wire. Reserved keys (those
//! beginning with `grpc-` or `:`) are managed by the protocol layer and
//! cannot be set through the public insertion API.

use anyhow::{bail, Result};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use std::collections::BTreeMap;

/// Default maximum metadata size: 8 KiB.
pub const MAX_METADATA_SIZE_DEFAULT: usize = 8 * 1024;

/// gRPC metadata: a map of keys to binary values (text values are stored as
/// UTF-8 bytes).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Metadata {
    map: BTreeMap<String, Vec<u8>>,
}

impl Metadata {
    /// Create an empty metadata set.
    pub fn new() -> Self {
        Metadata::default()
    }

    fn validate_key(key: &str, allow_reserved: bool) -> Result<()> {
        if key.is_empty() {
            bail!("empty metadata key");
        }
        if !allow_reserved && (key.starts_with("grpc-") || key.starts_with(':')) {
            bail!("reserved metadata key `{key}`");
        }
        for b in key.bytes() {
            if !(b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_' || b == b'.')
            {
                bail!("invalid metadata key `{key}`: keys must be lowercase ASCII");
            }
        }
        Ok(())
    }

    /// Insert a text (UTF-8) metadata value.
    pub fn insert_text(&mut self, key: &str, value: &str) -> Result<()> {
        Self::validate_key(key, false)?;
        self.insert_binary(key, value.as_bytes())
    }

    /// Insert a binary metadata value. Binary keys must end with `-bin`.
    pub fn insert_binary(&mut self, key: &str, value: &[u8]) -> Result<()> {
        Self::validate_key(key, false)?;
        if !key.ends_with("-bin") && value.iter().any(|&b| !b.is_ascii()) {
            bail!("non-ASCII value for non-binary key `{key}`");
        }
        self.map.insert(key.to_string(), value.to_vec());
        Ok(())
    }

    /// Insert a raw metadata value, bypassing the reserved-key restriction.
    ///
    /// Used internally to attach protocol-managed headers such as
    /// `grpc-status`.
    pub(crate) fn insert_raw(&mut self, key: &str, value: &[u8]) {
        self.map.insert(key.to_string(), value.to_vec());
    }

    /// Read a text metadata value.
    pub fn get_text(&self, key: &str) -> Option<&str> {
        self.map.get(key).and_then(|v| std::str::from_utf8(v).ok())
    }

    /// Read a binary metadata value.
    pub fn get_binary(&self, key: &str) -> Option<&[u8]> {
        self.map.get(key).map(|v| v.as_slice())
    }

    /// Whether a key is present.
    pub fn contains(&self, key: &str) -> bool {
        self.map.contains_key(key)
    }

    /// Iterate over keys in sorted order.
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.map.keys()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Total size in bytes (sum of key + value lengths).
    pub fn total_size(&self) -> usize {
        self.map.iter().map(|(k, v)| k.len() + v.len()).sum()
    }

    /// Parse a list of `(key, value)` header pairs into metadata.
    ///
    /// Reserved keys (`grpc-`, `:`) are rejected; use [`Metadata::from_trailers`]
    /// for trailers that carry protocol headers.
    pub fn from_headers(headers: &[(String, String)], max_size: usize) -> Result<Metadata> {
        Self::parse(headers, max_size, false)
    }

    /// Parse a list of `(key, value)` trailer pairs into metadata, allowing
    /// reserved `grpc-` keys.
    pub fn from_trailers(headers: &[(String, String)], max_size: usize) -> Result<Metadata> {
        Self::parse(headers, max_size, true)
    }

    fn parse(headers: &[(String, String)], max_size: usize, allow_reserved: bool) -> Result<Metadata> {
        let mut m = Metadata::new();
        for (k, v) in headers {
            Self::validate_key(k, allow_reserved)?;
            let value = if k.ends_with("-bin") {
                B64.decode(v)
                    .map_err(|e| anyhow::anyhow!("invalid base64 for binary metadata `{k}`: {e}"))?
            } else {
                v.clone().into_bytes()
            };
            m.map.insert(k.clone(), value);
        }
        if m.total_size() > max_size {
            bail!(
                "metadata size {} exceeds maximum {}",
                m.total_size(),
                max_size
            );
        }
        Ok(m)
    }

    /// Render metadata as wire header pairs, base64-encoding `-bin` values.
    pub fn to_headers(&self) -> Vec<(String, String)> {
        self.map
            .iter()
            .map(|(k, v)| {
                let value = if k.ends_with("-bin") {
                    B64.encode(v)
                } else {
                    String::from_utf8_lossy(v).into_owned()
                };
                (k.clone(), value)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_roundtrip() {
        let mut m = Metadata::new();
        m.insert_text("user-agent", "tpt-proto/0.1").unwrap();
        assert_eq!(m.get_text("user-agent"), Some("tpt-proto/0.1"));
    }

    #[test]
    fn binary_roundtrip() {
        let mut m = Metadata::new();
        m.insert_binary("auth-token-bin", b"\x01\x02\x03").unwrap();
        let headers = m.to_headers();
        let parsed = Metadata::from_headers(&headers, usize::MAX).unwrap();
        assert_eq!(parsed.get_binary("auth-token-bin"), Some(&[1, 2, 3][..]));
    }

    #[test]
    fn rejects_uppercase_key() {
        let mut m = Metadata::new();
        assert!(m.insert_text("User-Agent", "x").is_err());
    }

    #[test]
    fn rejects_reserved_key() {
        let mut m = Metadata::new();
        assert!(m.insert_text("grpc-status", "0").is_err());
    }

    #[test]
    fn allows_reserved_in_trailers() {
        let trailers = vec![("grpc-status".to_string(), "0".to_string())];
        let m = Metadata::from_trailers(&trailers, usize::MAX).unwrap();
        assert_eq!(m.get_text("grpc-status"), Some("0"));
    }

    #[test]
    fn enforces_size_limit() {
        let mut m = Metadata::new();
        m.insert_binary("big-bin", &vec![0u8; 100]).unwrap();
        assert!(Metadata::from_headers(&m.to_headers(), 10).is_err());
    }
}
