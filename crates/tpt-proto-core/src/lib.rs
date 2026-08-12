//! `tpt-proto-core` — binary wire format runtime for Protocol Buffers.
//!
//! This crate provides the low-level primitives used by the parser, compiler,
//! code generator, reflection, JSON, text, and gRPC layers: varint/zigzag
//! codecs, wire types and tags, a depth- and limit-aware [`Reader`], a
//! [`Writer`], per-scalar codecs, packed-field and map-entry helpers, unknown
//! field storage, and decode limits.
//!
//! It is dependency-light and contains no schema knowledge; higher layers
//! (descriptor, reflection, codegen) drive it.

mod error;
mod limits;
mod message;
mod packed;
mod reader;
pub mod scalar;
mod unknown;
mod varint;
mod wire_type;
mod writer;

#[cfg(feature = "bytes")]
pub use bytes;

pub use error::{Error, Result};
pub use limits::DecoderLimits;
pub use message::{decode_with_limits, DeterministicConfig, Message, UnknownFieldPolicy};
pub use packed::{
    decode_map_entry, encode_map_entry, encode_packed_fixed32, encode_packed_fixed64,
    encode_packed_varint, read_packed_fixed32, read_packed_fixed64, read_packed_varint,
};
pub use reader::Reader;
pub use unknown::{UnknownFieldSet, UnknownValue};
pub use varint::{decode_varint, encode_varint, encode_zigzag32, encode_zigzag64, decode_zigzag32, decode_zigzag64, MAX_VARINT_LEN};
pub use wire_type::{Tag, WireType};
pub use writer::Writer;
