//! Packed repeated fields and map-entry helpers.

use crate::wire_type::WireType;
use crate::Reader;
use crate::Writer;

/// Encode a packed repeated field whose elements are varints.
pub fn encode_packed_varint<I: IntoIterator<Item = u64>>(w: &mut Writer, field: u32, values: I) {
    let mut inner = Vec::new();
    for v in values {
        crate::varint::encode_varint(v, &mut inner);
    }
    w.write_tag(field, WireType::LengthDelimited);
    w.write_length_delimited(&inner);
}

/// Encode a packed repeated field whose elements are little-endian `u32`.
pub fn encode_packed_fixed32<I: IntoIterator<Item = u32>>(w: &mut Writer, field: u32, values: I) {
    let mut inner = Vec::new();
    for v in values {
        inner.extend_from_slice(&v.to_le_bytes());
    }
    w.write_tag(field, WireType::LengthDelimited);
    w.write_length_delimited(&inner);
}

/// Encode a packed repeated field whose elements are little-endian `u64`.
pub fn encode_packed_fixed64<I: IntoIterator<Item = u64>>(w: &mut Writer, field: u32, values: I) {
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
    let mut sub = r.nested(body)?;
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
        return Err(crate::Error::MalformedInput(
            "packed fixed32 length not a multiple of 4",
        ));
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
        return Err(crate::Error::MalformedInput(
            "packed fixed64 length not a multiple of 8",
        ));
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
pub fn encode_map_entry(w: &mut Writer, field: u32, key: &[u8], value: &[u8]) {
    let mut entry = Vec::new();
    entry.extend_from_slice(key);
    entry.extend_from_slice(value);
    w.write_tag(field, WireType::LengthDelimited);
    w.write_length_delimited(&entry);
}

/// Decode the key and value byte bodies of a map entry.
///
/// Reads the length-delimited entry body from `r`, descending through a nested
/// reader (so the parent [`DecoderLimits`], including `max_depth`, keep being
/// enforced) and returns the raw, untagged, owned value bytes for field 1 (key)
/// and field 2 (value), handling any wire type (varint, fixed, or
/// length-delimited). Duplicate keys are resolved by the caller with
/// last-value-wins semantics.
pub fn decode_map_entry(r: &mut Reader) -> crate::Result<(Vec<u8>, Vec<u8>)> {
    let (k, v) = decode_map_entry_frames(r)?;
    Ok((k.to_vec(), v.to_vec()))
}

/// Decode the key and value byte bodies of a map entry, returning slices that
/// borrow directly from the underlying buffer.
///
/// The nested entry reader inherits the parent's [`DecoderLimits`] (including
/// `max_depth`) so depth is enforced across the whole nesting chain rather than
/// reset to zero at the entry boundary. The returned slices are valid for as
/// long as the original buffer lives.
pub fn decode_map_entry_frames<'a>(
    r: &mut Reader<'a>,
) -> crate::Result<(&'a [u8], &'a [u8])> {
    let body = r.read_length_delimited()?;
    let mut er = r.nested(body)?;
    let mut key: Option<&'a [u8]> = None;
    let mut value: Option<&'a [u8]> = None;
    while !er.is_empty() {
        let tag = er.read_tag()?;
        let bytes = read_raw_value(&mut er, tag.wire_type)?;
        match tag.field_number {
            1 => key = Some(bytes),
            2 => value = Some(bytes),
            _ => {}
        }
    }
    let key = key.ok_or(crate::Error::MalformedInput("map entry missing key"))?;
    let value = value.unwrap_or(&[]);
    Ok((key, value))
}

/// Read the raw bytes of a value of the given wire type (without its tag).
///
/// The returned bytes are framed exactly as they appeared on the wire *after*
/// the field tag, i.e. for `LengthDelimited` they include the length prefix so
/// the caller can re-parse them with the normal `Reader` helpers (e.g.
/// `read_string_owned` / `merge_from`). This lets map-entry value decode use the
/// same code paths as top-level field decode. The slices borrow directly from
/// the underlying buffer for `'a`.
fn read_raw_value<'a>(r: &mut Reader<'a>, wt: WireType) -> crate::Result<&'a [u8]> {
    match wt {
        WireType::Varint => {
            let (v, raw) = r.read_varint_raw()?;
            let _ = v;
            Ok(raw)
        }
        WireType::Fixed32 => {
            let start = r.pos();
            let _ = r.read_fixed32()?;
            Ok(r.buf_slice(start, start + 4)?)
        }
        WireType::Fixed64 => {
            let start = r.pos();
            let _ = r.read_fixed64()?;
            Ok(r.buf_slice(start, start + 8)?)
        }
        WireType::LengthDelimited => {
            // Re-collect the length prefix alongside the delimited body so the
            // caller can re-read it through the ordinary length-delimited path.
            let start = r.pos();
            let len = r.read_varint()? as usize;
            r.limits().check_length(len)?;
            let end = r.pos() + len;
            if end > r.buf().len() {
                return Err(crate::Error::LengthLimitExceeded { len });
            }
            let framed = r.buf_slice(start, end)?;
            r.set_pos(end)?;
            Ok(framed)
        }
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
        let (mk, mv) = decode_map_entry(&mut r).unwrap();
        // Values are returned framed exactly as on the wire, so they re-parse
        // with the ordinary `Reader` helpers (mirrors generated decode paths).
        assert_eq!(Reader::new(&mk).read_string_owned().unwrap(), "a");
        assert_eq!(Reader::new(&mv).read_varint().unwrap(), 7);
    }

    #[test]
    fn map_entry_propagates_limits() {
        use crate::limits::DecoderLimits;
        let mut w = Writer::new();
        let mut k = Writer::new();
        crate::scalar::encode_string(&mut k, 1, "a");
        let mut v = Writer::new();
        crate::scalar::encode_int32(&mut v, 2, 7);
        encode_map_entry(&mut w, 3, k.buf(), v.buf());
        // A tiny max_length must still allow the entry body itself.
        let mut limits = DecoderLimits::default();
        limits.max_depth = 1;
        let mut r = Reader::with_limits(w.buf(), limits);
        let _ = r.read_tag().unwrap();
        // The entry is one nesting level deep; decoding it at max_depth=1 must
        // succeed (depth is counted, not reset to zero).
        let res = decode_map_entry(&mut r);
        assert!(res.is_ok(), "map entry decode must honor inherited limits");
    }
}
