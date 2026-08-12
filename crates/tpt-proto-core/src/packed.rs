//! Packed repeated fields and map-entry helpers.

use crate::wire_type::WireType;
use crate::Reader;
use crate::Writer;

/// Encode a packed repeated field whose elements are varints.
pub fn encode_packed_varint<I: IntoIterator<Item = u64>>(
    w: &mut Writer,
    field: u32,
    values: I,
) {
    let mut inner = Vec::new();
    for v in values {
        crate::varint::encode_varint(v, &mut inner);
    }
    w.write_tag(field, WireType::LengthDelimited);
    w.write_length_delimited(&inner);
}

/// Encode a packed repeated field whose elements are little-endian `u32`.
pub fn encode_packed_fixed32<I: IntoIterator<Item = u32>>(
    w: &mut Writer,
    field: u32,
    values: I,
) {
    let mut inner = Vec::new();
    for v in values {
        inner.extend_from_slice(&v.to_le_bytes());
    }
    w.write_tag(field, WireType::LengthDelimited);
    w.write_length_delimited(&inner);
}

/// Encode a packed repeated field whose elements are little-endian `u64`.
pub fn encode_packed_fixed64<I: IntoIterator<Item = u64>>(
    w: &mut Writer,
    field: u32,
    values: I,
) {
    let mut inner = Vec::new();
    for v in values {
        inner.extend_from_slice(&v.to_le_bytes());
    }
    w.write_tag(field, WireType::LengthDelimited);
    w.write_length_delimited(&inner);
}

/// Read a packed varint field body (the bytes after the length prefix).
///
/// `read_one` decodes a single varint from the sub-reader and returns the
/// element; it is called once per element until the slice is exhausted.
pub fn read_packed_varint<F, T>(r: &mut Reader, read_one: F) -> crate::Result<Vec<T>>
where
    F: Fn(&mut Reader) -> crate::Result<T>,
{
    let body = r.read_length_delimited()?;
    let mut sub = Reader::new(body);
    let mut out = Vec::new();
    while !sub.is_empty() {
        out.push(read_one(&mut sub)?);
    }
    Ok(out)
}

/// Read a packed fixed32 field body into a `Vec<u32>` (4 bytes each).
pub fn read_packed_fixed32(r: &mut Reader) -> crate::Result<Vec<u32>> {
    let body = r.read_length_delimited()?;
    if body.len() % 4 != 0 {
        return Err(crate::Error::MalformedInput("packed fixed32 length not a multiple of 4"));
    }
    let mut out = Vec::with_capacity(body.len() / 4);
    let mut sub = Reader::new(body);
    while !sub.is_empty() {
        out.push(sub.read_fixed32()?);
    }
    Ok(out)
}

/// Read a packed fixed64 field body into a `Vec<u64>` (8 bytes each).
pub fn read_packed_fixed64(r: &mut Reader) -> crate::Result<Vec<u64>> {
    let body = r.read_length_delimited()?;
    if body.len() % 8 != 0 {
        return Err(crate::Error::MalformedInput("packed fixed64 length not a multiple of 8"));
    }
    let mut out = Vec::with_capacity(body.len() / 8);
    let mut sub = Reader::new(body);
    while !sub.is_empty() {
        out.push(sub.read_fixed64()?);
    }
    Ok(out)
}

/// Encode a single map entry (key=field 1, value=field 2) as a length-delimited
/// sub-message. `key`/`value` encode the raw (already tagged) field bodies.
pub fn encode_map_entry(
    w: &mut Writer,
    field: u32,
    key: &[u8],
    value: &[u8],
) {
    let mut entry = Vec::new();
    entry.extend_from_slice(key);
    entry.extend_from_slice(value);
    w.write_tag(field, WireType::LengthDelimited);
    w.write_length_delimited(&entry);
}

/// Decode the key and value byte bodies of a map entry.
///
/// Returns the raw, untagged value bytes for field 1 (key) and field 2 (value),
/// handling any wire type (varint, fixed, or length-delimited). Duplicate keys
/// are resolved by the caller with last-value-wins semantics.
pub fn decode_map_entry(body: &[u8]) -> crate::Result<(Vec<u8>, Vec<u8>)> {
    let mut r = Reader::new(body);
    let mut key = None;
    let mut value = None;
    while !r.is_empty() {
        let tag = r.read_tag()?;
        let bytes = read_raw_value(&mut r, tag.wire_type)?;
        match tag.field_number {
            1 => key = Some(bytes),
            2 => value = Some(bytes),
            _ => {}
        }
    }
    let key = key.ok_or(crate::Error::MalformedInput("map entry missing key"))?;
    let value = value.unwrap_or_default();
    Ok((key, value))
}

/// Read the raw bytes of a value of the given wire type (without its tag).
fn read_raw_value(r: &mut Reader, wt: WireType) -> crate::Result<Vec<u8>> {
    match wt {
        WireType::Varint => {
            let (v, raw) = r.read_varint_raw()?;
            let _ = v;
            Ok(raw.to_vec())
        }
        WireType::Fixed32 => {
            let v = r.read_fixed32()?;
            Ok(v.to_le_bytes().to_vec())
        }
        WireType::Fixed64 => {
            let v = r.read_fixed64()?;
            Ok(v.to_le_bytes().to_vec())
        }
        WireType::LengthDelimited => Ok(r.read_length_delimited()?.to_vec()),
        WireType::StartGroup | WireType::EndGroup => {
            Err(crate::Error::UnexpectedWireType { found: wt.as_u8() })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_varint_roundtrip() {
        let mut w = Writer::new();
        encode_packed_varint(&mut w, 1, [1u64, 2, 300, 4]);
        let mut r = Reader::new(w.buf());
        let tag = r.read_tag().unwrap();
        assert_eq!(tag.field_number, 1);
        let vals = read_packed_varint(&mut r, |s| s.read_varint()).unwrap();
        assert_eq!(vals, vec![1u64, 2, 300, 4]);
    }

    #[test]
    fn packed_fixed32_roundtrip() {
        let mut w = Writer::new();
        encode_packed_fixed32(&mut w, 2, [10u32, 20, 30]);
        let mut r = Reader::new(w.buf());
        let _ = r.read_tag().unwrap();
        assert_eq!(r.read_length_delimited().unwrap().len(), 12);
        let mut r2 = Reader::new(w.buf());
        let _ = r2.read_tag().unwrap();
        assert_eq!(read_packed_fixed32(&mut r2).unwrap(), vec![10u32, 20, 30]);
    }

    #[test]
    fn map_entry_roundtrip() {
        let mut w = Writer::new();
        // key = "a" (field 1), value = 7 (field 2, varint)
        let mut k = Writer::new();
        crate::scalar::encode_string(&mut k, 1, "a");
        let mut v = Writer::new();
        crate::scalar::encode_int32(&mut v, 2, 7);
        encode_map_entry(&mut w, 3, k.buf(), v.buf());
        let mut r = Reader::new(w.buf());
        let _ = r.read_tag().unwrap();
        let body = r.read_length_delimited().unwrap();
        let (mk, mv) = decode_map_entry(body).unwrap();
        assert_eq!(mk, b"a");
        assert_eq!(mv, vec![7u8]);
    }
}
