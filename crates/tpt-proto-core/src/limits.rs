//! Decode limits to bound resource usage while parsing untrusted input.

/// Limits applied while decoding messages from untrusted input.
///
/// All fields are conservative by default. Set [`DecoderLimits::unlimited`]
/// only for trusted, fully-local input.
#[derive(Debug, Clone, Copy)]
pub struct DecoderLimits {
    /// Maximum total bytes the reader may consume across a single top-level
    /// decode call (and all nested messages). `None` for no cap.
    pub max_bytes: Option<usize>,
    /// Maximum nesting depth of embedded messages / groups.
    pub max_depth: u32,
    /// Maximum number of fields (tags) that may be read.
    pub max_fields: Option<usize>,
    /// Maximum length of any single length-delimited value.
    pub max_length_delimited: Option<usize>,
    /// Maximum length of any string field (UTF-8). `None` allows any length
    /// permitted by `max_length_delimited`.
    pub max_string_len: Option<usize>,
    /// Maximum number of bytes allocated for unknown fields.
    pub max_unknown_bytes: Option<usize>,
}

impl Default for DecoderLimits {
    fn default() -> Self {
        DecoderLimits {
            max_bytes: Some(64 * 1024 * 1024),
            max_depth: 100,
            max_fields: Some(1_000_000),
            max_length_delimited: Some(16 * 1024 * 1024),
            max_string_len: Some(16 * 1024 * 1024),
            max_unknown_bytes: Some(16 * 1024 * 1024),
        }
    }
}

impl DecoderLimits {
    /// Limits that never reject input. Intended only for trusted input.
    pub const fn unlimited() -> Self {
        DecoderLimits {
            max_bytes: None,
            max_depth: u32::MAX,
            max_fields: None,
            max_length_delimited: None,
            max_string_len: None,
            max_unknown_bytes: None,
        }
    }

    /// Returns `Err(LimitExceeded)` if `bytes` would exceed `max_bytes`.
    pub(crate) fn check_bytes(&self, total: usize) -> crate::Result<()> {
        if let Some(cap) = self.max_bytes {
            if total > cap {
                return Err(crate::Error::LimitExceeded("max_bytes"));
            }
        }
        Ok(())
    }

    pub(crate) fn check_field(&self, count: usize) -> crate::Result<()> {
        if let Some(cap) = self.max_fields {
            if count > cap {
                return Err(crate::Error::LimitExceeded("max_fields"));
            }
        }
        Ok(())
    }

    pub(crate) fn check_length(&self, len: usize) -> crate::Result<()> {
        if let Some(cap) = self.max_length_delimited {
            if len > cap {
                return Err(crate::Error::LimitExceeded("max_length_delimited"));
            }
        }
        Ok(())
    }

    pub(crate) fn check_string(&self, len: usize) -> crate::Result<()> {
        if let Some(cap) = self.max_string_len {
            if len > cap {
                return Err(crate::Error::LimitExceeded("max_string_len"));
            }
        }
        Ok(())
    }

    pub(crate) fn check_unknown(&self, total: usize) -> crate::Result<()> {
        if let Some(cap) = self.max_unknown_bytes {
            if total > cap {
                return Err(crate::Error::LimitExceeded("max_unknown_bytes"));
            }
        }
        Ok(())
    }
}

