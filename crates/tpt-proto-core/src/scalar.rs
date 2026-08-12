//! Scalar value codecs for each protobuf scalar type.

use crate::varint::{decode_zigzag32, decode_zigzag64, encode_zigzag32, encode_zigzag64};
use crate::wire_type::WireType;
use crate::Reader;
use crate::Writer;

macro_rules! varint_field {
    ($name:ident, $ty:ty, $cast:ty) => {
        /// Encode a varint scalar field (tag + value).
        pub fn $name(w: &mut Writer, field: u32, value: $ty) {
            w.write_tag(field, WireType::Varint);
            w.write_varint(value as $cast);
        }
    };
}

varint_field!(encode_int32, i32, u64);
varint_field!(encode_int64, i64, u64);
varint_field!(encode_uint32, u32, u64);
varint_field!(encode_uint64, u64, u64);
varint_field!(encode_bool, bool, u64);

/// Encode an `sint32` (zigzag) field.
pub fn encode_sint32(w: &mut Writer, field: u32, value: i32) {
    w.write_tag(field, WireType::Varint);
    w.write_varint(encode_zigzag32(value) as u64);
}

/// Encode an `sint64` (zigzag) field.
pub fn encode_sint64(w: &mut Writer, field: u32, value: i64) {
    w.write_tag(field, WireType::Varint);
    w.write_varint(encode_zigzag64(value));
}

/// Encode an `enum` field (as its `i32` value).
pub fn encode_enum(w: &mut Writer, field: u32, value: i32) {
    encode_int32(w, field, value);
}

/// Encode a `fixed32` field.
pub fn encode_fixed32(w: &mut Writer, field: u32, value: u32) {
    w.write_tag(field, WireType::Fixed32);
    w.write_fixed32(value);
}

/// Encode an `sfixed32` field.
pub fn encode_sfixed32(w: &mut Writer, field: u32, value: i32) {
    encode_fixed32(w, field, value as u32);
}

/// Encode a `fixed64` field.
pub fn encode_fixed64(w: &mut Writer, field: u32, value: u64) {
    w.write_tag(field, WireType::Fixed64);
    w.write_fixed64(value);
}

/// Encode an `sfixed64` field.
pub fn encode_sfixed64(w: &mut Writer, field: u32, value: i64) {
    encode_fixed64(w, field, value as u64);
}

/// Encode a `float` field.
pub fn encode_float(w: &mut Writer, field: u32, value: f32) {
    encode_fixed32(w, field, value.to_bits());
}

/// Encode a `double` field.
pub fn encode_double(w: &mut Writer, field: u32, value: f64) {
    encode_fixed64(w, field, value.to_bits());
}

/// Encode a `string` field.
pub fn encode_string(w: &mut Writer, field: u32, value: &str) {
    w.write_tag(field, WireType::LengthDelimited);
    w.write_string(value);
}

/// Encode a `bytes` field.
pub fn encode_bytes(w: &mut Writer, field: u32, value: &[u8]) {
    w.write_tag(field, WireType::LengthDelimited);
    w.write_length_delimited(value);
}

/// Encode a nested message field (length-delimited).
pub fn encode_message(w: &mut Writer, field: u32, value: &[u8]) {
    encode_bytes(w, field, value);
}

// ---- Decode helpers (assume the tag has already been consumed) ----

/// Read a varint-decoded signed value as `i32`.
pub fn read_int32(r: &mut Reader) -> crate::Result<i32> {
    Ok(r.read_varint()? as i32)
}

/// Read a varint-decoded value as `i64`.
pub fn read_int64(r: &mut Reader) -> crate::Result<i64> {
    Ok(r.read_varint()? as i64)
}

/// Read a varint-decoded value as `u32`.
pub fn read_uint32(r: &mut Reader) -> crate::Result<u32> {
    Ok(r.read_varint()? as u32)
}

/// Read a varint-decoded value as `u64`.
pub fn read_uint64(r: &mut Reader) -> crate::Result<u64> {
    r.read_varint()
}

/// Read a varint-decoded boolean.
pub fn read_bool(r: &mut Reader) -> crate::Result<bool> {
    Ok(r.read_varint()? != 0)
}

/// Read a zigzag-decoded `i32`.
pub fn read_sint32(r: &mut Reader) -> crate::Result<i32> {
    Ok(decode_zigzag32(r.read_varint()? as u32))
}

/// Read a zigzag-decoded `i64`.
pub fn read_sint64(r: &mut Reader) -> crate::Result<i64> {
    Ok(decode_zigzag64(r.read_varint()?))
}

/// Read a fixed32 as `u32`.
pub fn read_fixed32(r: &mut Reader) -> crate::Result<u32> {
    r.read_fixed32()
}

/// Read a fixed32 as `i32`.
pub fn read_sfixed32(r: &mut Reader) -> crate::Result<i32> {
    Ok(r.read_fixed32()? as i32)
}

/// Read a fixed64 as `u64`.
pub fn read_fixed64(r: &mut Reader) -> crate::Result<u64> {
    r.read_fixed64()
}

/// Read a fixed64 as `i64`.
pub fn read_sfixed64(r: &mut Reader) -> crate::Result<i64> {
    Ok(r.read_fixed64()? as i64)
}

/// Read a `float`.
pub fn read_float(r: &mut Reader) -> crate::Result<f32> {
    Ok(f32::from_bits(r.read_fixed32()?))
}

/// Read a `double`.
pub fn read_double(r: &mut Reader) -> crate::Result<f64> {
    Ok(f64::from_bits(r.read_fixed64()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int32_negative_roundtrip() {
        let mut w = Writer::new();
        encode_int32(&mut w, 1, -1);
        let mut r = Reader::new(w.buf());
        let tag = r.read_tag().unwrap();
        assert_eq!(tag.field_number, 1);
        assert_eq!(read_int32(&mut r).unwrap(), -1);
    }

    #[test]
    fn sint32_roundtrip() {
        for v in [i32::MIN, -1, 0, 1, i32::MAX] {
            let mut w = Writer::new();
            encode_sint32(&mut w, 1, v);
            let mut r = Reader::new(w.buf());
            let _ = r.read_tag().unwrap();
            assert_eq!(read_sint32(&mut r).unwrap(), v);
        }
    }

    #[test]
    fn fixed_and_string_roundtrip() {
        let mut w = Writer::new();
        encode_fixed64(&mut w, 1, 0x0102030405060708);
        encode_string(&mut w, 2, "héllo");
        encode_bytes(&mut w, 3, &[1, 2, 3]);
        let mut r = Reader::new(w.buf());
        let _t1 = r.read_tag().unwrap();
        assert_eq!(read_fixed64(&mut r).unwrap(), 0x0102030405060708);
        let t2 = r.read_tag().unwrap();
        assert_eq!(r.read_string().unwrap(), "héllo");
        assert_eq!(t2.field_number, 2);
        let t3 = r.read_tag().unwrap();
        assert_eq!(r.read_length_delimited().unwrap(), &[1, 2, 3]);
        assert_eq!(t3.field_number, 3);
    }
}
