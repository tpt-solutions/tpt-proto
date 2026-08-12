//! Low-level encoding writer.

use crate::varint;
use crate::wire_type::Tag;

/// A growable writer that appends protobuf wire-format bytes.
///
/// Buffers are reused across encodes where possible; callers typically keep a
/// `Writer` (or its backing `Vec`) per message encode.
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    /// Create a writer with the default capacity.
    pub fn new() -> Self {
        Writer { buf: Vec::new() }
    }

    /// Create a writer with a pre-allocated capacity.
    pub fn with_capacity(cap: usize) -> Self {
        Writer {
            buf: Vec::with_capacity(cap),
        }
    }

    /// Borrow the underlying buffer.
    pub fn buf(&self) -> &[u8] {
        &self.buf
    }

    /// Consume the writer, returning the buffered bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }

    /// Clear the buffer, retaining allocated capacity.
    pub fn clear(&mut self) {
        self.buf.clear();
    }

    /// Append a raw byte.
    pub fn push(&mut self, byte: u8) {
        self.buf.push(byte);
    }

    /// Append a raw slice.
    pub fn extend_from_slice(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// Write a tag for `field_number` with the given wire type.
    pub fn write_tag(&mut self, field_number: u32, wire_type: crate::wire_type::WireType) {
        Tag::new(field_number, wire_type).expect("field number must be non-zero").encode(&mut self.buf);
    }

    /// Write a `u64` varint.
    pub fn write_varint(&mut self, value: u64) {
        varint::encode_varint(value, &mut self.buf);
    }

    /// Write a length prefix followed by `bytes`.
    pub fn write_length_delimited(&mut self, bytes: &[u8]) {
        varint::encode_varint(bytes.len() as u64, &mut self.buf);
        self.buf.extend_from_slice(bytes);
    }

    /// Write a little-endian `u32` (wire type 5).
    pub fn write_fixed32(&mut self, value: u32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    /// Write a little-endian `u64` (wire type 1).
    pub fn write_fixed64(&mut self, value: u64) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    /// Write a length-delimited UTF-8 string (caller guarantees validity).
    pub fn write_string(&mut self, value: &str) {
        self.write_length_delimited(value.as_bytes());
    }
}

impl Default for Writer {
    fn default() -> Self {
        Writer::new()
    }
}
