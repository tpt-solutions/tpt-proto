//! Low-level decoding reader with depth and limit tracking.

use crate::error::Error;
use crate::limits::DecoderLimits;
use crate::varint;
use crate::wire_type::{Tag, WireType};

/// A cursor over a protobuf wire-format byte slice.
///
/// Tracks the current position, nesting depth, and field count so that
/// [`DecoderLimits`] can be enforced. Unknown-field storage is performed by
/// higher-level message code via [`Reader::read_length_delimited`].
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
    depth: u32,
    field_count: usize,
    limits: DecoderLimits,
    bytes_read: usize,
}

impl<'a> Reader<'a> {
    /// Create a reader for `buf` using [`DecoderLimits::default`].
    pub fn new(buf: &'a [u8]) -> Self {
        Self::with_limits(buf, DecoderLimits::default())
    }

    /// Create a reader with explicit limits.
    pub fn with_limits(buf: &'a [u8], limits: DecoderLimits) -> Self {
        Reader {
            buf,
            pos: 0,
            depth: 0,
            field_count: 0,
            limits,
            bytes_read: 0,
        }
    }

    /// The configured decode limits.
    pub fn limits(&self) -> &DecoderLimits {
        &self.limits
    }

    /// The underlying buffer slice this reader reads from.
    pub fn buf(&self) -> &'a [u8] {
        self.buf
    }

    /// The current absolute position within [`Reader::buf`].
    pub fn set_pos(&mut self, pos: usize) -> crate::Result<()> {
        if pos > self.buf.len() {
            return Err(crate::Error::UnexpectedEof);
        }
        self.pos = pos;
        Ok(())
    }

    /// Return a sub-slice `[start..end)` of the underlying buffer.
    pub fn buf_slice(&self, start: usize, end: usize) -> crate::Result<&'a [u8]> {
        if end > self.buf.len() || start > end {
            return Err(crate::Error::UnexpectedEof);
        }
        Ok(&self.buf[start..end])
    }

    /// Bytes remaining in the buffer.
    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    /// Whether the buffer is fully consumed.
    pub fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }

    /// Current absolute byte position within the buffer.
    pub fn pos(&self) -> usize {
        self.pos
    }

    fn advance(&mut self, n: usize) -> Result<(), Error> {
        if self.pos + n > self.buf.len() {
            return Err(Error::UnexpectedEof);
        }
        self.pos += n;
        self.bytes_read += n;
        self.limits.check_bytes(self.bytes_read)?;
        Ok(())
    }

    /// Read a raw varint and return its value.
    pub fn read_varint(&mut self) -> crate::Result<u64> {
        let (value, n) = varint::decode_varint(self.buf, self.pos)?;
        self.advance(n)?;
        Ok(value)
    }

    /// Read a raw varint, returning both its value and the exact byte slice it
    /// occupied (useful for re-emitting values verbatim, e.g. map entry keys).
    pub fn read_varint_raw(&mut self) -> crate::Result<(u64, &'a [u8])> {
        let (value, n) = varint::decode_varint(self.buf, self.pos)?;
        let slice = &self.buf[self.pos..self.pos + n];
        self.advance(n)?;
        Ok((value, slice))
    }

    /// Read and decode the next [`Tag`].
    pub fn read_tag(&mut self) -> crate::Result<Tag> {
        let (tag, n) = Tag::decode(self.buf, self.pos)?;
        self.advance(n)?;
        Ok(tag)
    }

    /// Read a length prefix and return a sub-slice of exactly that length.
    pub fn read_length_delimited(&mut self) -> crate::Result<&'a [u8]> {
        let len = self.read_varint()? as usize;
        self.limits.check_length(len)?;
        if self.pos + len > self.buf.len() {
            return Err(Error::LengthLimitExceeded { len });
        }
        let slice = &self.buf[self.pos..self.pos + len];
        self.advance(len)?;
        Ok(slice)
    }

    /// Read a little-endian `u32` (wire type 5).
    pub fn read_fixed32(&mut self) -> crate::Result<u32> {
        if self.pos + 4 > self.buf.len() {
            return Err(Error::UnexpectedEof);
        }
        let bytes: [u8; 4] = self.buf[self.pos..self.pos + 4].try_into().unwrap();
        self.advance(4)?;
        Ok(u32::from_le_bytes(bytes))
    }

    /// Read a little-endian `u64` (wire type 1).
    pub fn read_fixed64(&mut self) -> crate::Result<u64> {
        if self.pos + 8 > self.buf.len() {
            return Err(Error::UnexpectedEof);
        }
        let bytes: [u8; 8] = self.buf[self.pos..self.pos + 8].try_into().unwrap();
        self.advance(8)?;
        Ok(u64::from_le_bytes(bytes))
    }

    /// Read a length-delimited value as a UTF-8 string, validating encoding.
    pub fn read_string(&mut self) -> crate::Result<&'a str> {
        let bytes = self.read_length_delimited()?;
        self.limits.check_string(bytes.len())?;
        std::str::from_utf8(bytes)?;
        // SAFETY: validated as UTF-8 above.
        Ok(unsafe { std::str::from_utf8_unchecked(bytes) })
    }

    /// Read a length-delimited value as an owned [`String`], validating UTF-8.
    pub fn read_string_owned(&mut self) -> crate::Result<String> {
        let bytes = self.read_length_delimited()?;
        self.limits.check_string(bytes.len())?;
        Ok(String::from_utf8(bytes.to_vec())?)
    }

    /// Create a sub-reader over an already-extracted `buf` (typically a
    /// length-delimited payload returned by [`Reader::read_length_delimited`])
    /// that inherits this reader's [`DecoderLimits`] and continues its nesting
    /// `depth`. This is what allows [`DecoderLimits::max_depth`] to be enforced
    /// across the entire nesting chain rather than reset to zero at each level.
    ///
    /// The caller must have already advanced this reader past `buf` (which
    /// `read_length_delimited` does) so the parent's byte/depth accounting is
    /// preserved.
    pub fn nested(&self, buf: &'a [u8]) -> crate::Result<Reader<'a>> {
        let depth = self.depth + 1;
        if depth > self.limits.max_depth {
            return Err(Error::DepthLimitExceeded);
        }
        Ok(Reader {
            buf,
            pos: 0,
            depth,
            field_count: 0,
            limits: self.limits,
            bytes_read: 0,
        })
    }

    /// Enter a nested message (after the caller has read its length prefix).
    /// Returns a sub-reader bounded to `len` bytes.
    pub fn enter_message(&mut self, len: usize) -> crate::Result<Reader<'a>> {
        self.limits.check_length(len)?;
        if self.pos + len > self.buf.len() {
            return Err(Error::LengthLimitExceeded { len });
        }
        let slice = &self.buf[self.pos..self.pos + len];
        self.advance(len)?;
        self.depth += 1;
        if self.depth > self.limits.max_depth {
            return Err(Error::DepthLimitExceeded);
        }
        Ok(Reader {
            buf: slice,
            pos: 0,
            depth: 0,
            field_count: 0,
            limits: self.limits,
            bytes_read: 0,
        })
    }

    /// Record that a field has been consumed (for `max_fields` accounting).
    pub fn record_field(&mut self) -> crate::Result<()> {
        self.field_count += 1;
        self.limits.check_field(self.field_count)
    }

    /// Skip a value of the given wire type, correctly descending into groups.
    pub fn skip(&mut self, wire_type: WireType) -> crate::Result<()> {
        self.skip_inner(wire_type, self.depth)
    }

    /// Recursive helper for [`Reader::skip`] that tracks nesting `depth` so the
    /// `max_depth` limit is enforced when skipping legacy groups (groups can
    /// nest arbitrarily and would otherwise recurse without bound).
    fn skip_inner(&mut self, wire_type: WireType, depth: u32) -> crate::Result<()> {
        match wire_type {
            WireType::Varint => {
                self.read_varint()?;
            }
            WireType::Fixed64 => {
                self.read_fixed64()?;
            }
            WireType::Fixed32 => {
                self.read_fixed32()?;
            }
            WireType::LengthDelimited => {
                let len = self.read_varint()? as usize;
                self.limits.check_length(len)?;
                self.advance(len)?;
            }
            WireType::StartGroup => {
                // Consume until the matching EndGroup tag, enforcing depth.
                let child_depth = depth + 1;
                if child_depth > self.limits.max_depth {
                    return Err(Error::DepthLimitExceeded);
                }
                loop {
                    if self.is_empty() {
                        return Err(Error::UnterminatedGroup(0));
                    }
                    let tag = self.read_tag()?;
                    if tag.wire_type == WireType::EndGroup {
                        break;
                    }
                    self.skip_inner(tag.wire_type, child_depth)?;
                }
            }
            WireType::EndGroup => {
                return Err(Error::UnexpectedWireType { found: 4 });
            }
        }
        Ok(())
    }
}
