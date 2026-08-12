//! The `Message` contract, unknown-field policy, and deterministic encoding.

use crate::error::Error;
use crate::limits::DecoderLimits;
use crate::Reader;
use crate::Writer;

/// Policy for handling fields not present in the schema during decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnknownFieldPolicy {
    /// Preserve unknown fields so they can be re-emitted on encode (default).
    #[default]
    Preserve,
    /// Discard unknown fields.
    Discard,
    /// Fail the decode if any unknown field is encountered.
    Fail,
}

/// Deterministic encoding controls.
///
/// When enabled, fields are emitted in ascending field-number order, map
/// entries are sorted by key, and canonical varints are used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DeterministicConfig {
    /// Enforce deterministic field/map ordering.
    pub enabled: bool,
    /// Enforce canonical varints (no overlong varints). The runtime always
    /// emits minimal varints, so this is effectively always true for output.
    pub canonical_varints: bool,
}

/// Core trait implemented by generated messages and dynamic messages.
pub trait Message {
    /// Encode this message, appending to `w`.
    fn encode(&self, w: &mut Writer) -> Result<(), Error>;

    /// Decode a message from `r`, replacing `self`'s fields.
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error>;

    /// Decode a fresh message from a byte slice.
    fn decode(bytes: &[u8]) -> Result<Self, Error>
    where
        Self: Default + Sized,
    {
        let mut msg = Self::default();
        let mut r = Reader::with_limits(bytes, DecoderLimits::default());
        msg.merge_from(&mut r)?;
        if !r.is_empty() {
            return Err(Error::MalformedInput("trailing bytes after message"));
        }
        Ok(msg)
    }

    /// Encode this message to a fresh `Vec`.
    fn encode_to_vec(&self) -> Result<Vec<u8>, Error> {
        let mut w = Writer::new();
        self.encode(&mut w)?;
        Ok(w.into_vec())
    }
}

/// Convenience decode with custom limits.
pub fn decode_with_limits<M: Message + Default>(
    bytes: &[u8],
    limits: DecoderLimits,
) -> Result<M, Error> {
    let mut msg = M::default();
    let mut r = Reader::with_limits(bytes, limits);
    msg.merge_from(&mut r)?;
    Ok(msg)
}
