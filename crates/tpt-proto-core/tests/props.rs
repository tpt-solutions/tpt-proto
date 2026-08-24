//! Property-based tests: random scalar/packed/map payloads must round-trip
//! through the wire codecs (encode → decode → re-encode identity).

use proptest::prelude::*;
use tpt_proto_core::{
    decode_zigzag32, decode_zigzag64, encode_zigzag32, encode_zigzag64, packed, scalar, Reader,
    Writer,
};

proptest! {
    /// Varints are the canonical encoding: decoding reproduces the value, and
    /// re-encoding the decoded value yields identical bytes.
    #[test]
    fn varint_roundtrip(v in any::<u64>()) {
        let mut w = Writer::new();
        w.write_varint(v);
        let bytes = w.buf().to_vec();
        let mut r = Reader::new(&bytes);
        let back = r.read_varint().unwrap();
        prop_assert_eq!(back, v);
        let mut w2 = Writer::new();
        w2.write_varint(back);
        prop_assert_eq!(w2.buf(), &bytes[..]);
    }

    /// Zigzag maps i32/i64 bijectively onto u32/u64.
    #[test]
    fn zigzag32_roundtrip(v in any::<i32>()) {
        prop_assert_eq!(decode_zigzag32(encode_zigzag32(v)), v);
    }

    #[test]
    fn zigzag64_roundtrip(v in any::<i64>()) {
        prop_assert_eq!(decode_zigzag64(encode_zigzag64(v)), v);
    }

    /// Negative int32 values must occupy a full 10-byte varint on the wire
    /// (sign-extension rule from the public encoding spec).
    #[test]
    fn negative_int32_is_ten_bytes(v in -1_000_000i32..=-1) {
        let mut w = Writer::new();
        scalar::encode_int32(&mut w, 1, v);
        // tag(1 byte) + varint(10 bytes)
        prop_assert_eq!(w.buf().len(), 11, "bytes: {:02x?}", w.buf());
        let mut r = Reader::new(w.buf());
        let _tag = r.read_tag().unwrap();
        prop_assert_eq!(scalar::read_int32(&mut r).unwrap(), v);
    }

    #[test]
    fn fixed32_roundtrip(v in any::<u32>()) {
        let mut w = Writer::new();
        scalar::encode_fixed32(&mut w, 7, v);
        let mut r = Reader::new(w.buf());
        let tag = r.read_tag().unwrap();
        prop_assert_eq!((7, tpt_proto_core::WireType::Fixed32), (tag.field_number, tag.wire_type));
        prop_assert_eq!(r.read_fixed32().unwrap(), v);
    }

    #[test]
    fn fixed64_roundtrip(v in any::<u64>()) {
        let mut w = Writer::new();
        scalar::encode_fixed64(&mut w, 8, v);
        let mut r = Reader::new(w.buf());
        let tag = r.read_tag().unwrap();
        prop_assert_eq!(8, tag.field_number);
        prop_assert_eq!(r.read_fixed64().unwrap(), v);
    }

    /// Strings survive encode/decode byte-for-byte, including multi-byte UTF-8.
    #[test]
    fn string_roundtrip(s in ".{0,128}") {
        let mut w = Writer::new();
        scalar::encode_string(&mut w, 2, s.as_str());
        let mut r = Reader::new(w.buf());
        let _tag = r.read_tag().unwrap();
        let body = r.read_length_delimited().unwrap();
        let decoded = String::from_utf8_lossy(body);
        prop_assert_eq!(decoded.as_ref(), s.as_str());
    }

    /// Packed varint lists: decode returns exactly the encoded elements.
    #[test]
    fn packed_varint_roundtrip(vals in prop::collection::vec(any::<u64>(), 0..64)) {
        let mut w = Writer::new();
        packed::encode_packed_varint(&mut w, 3, vals.iter().copied());
        let mut r = Reader::new(w.buf());
        let _tag = r.read_tag().unwrap();
        let out: Vec<u64> =
            packed::read_packed_varint(&mut r, |s| s.read_varint()).unwrap();
        prop_assert_eq!(out, vals);
    }

    /// Map entries: key/value bodies are preserved verbatim.
    #[test]
    fn map_entry_roundtrip(k in "[a-z]{0,24}", v in any::<i64>()) {
        let mut kw = Writer::new();
        scalar::encode_string(&mut kw, 1, k.as_str());
        let mut vw = Writer::new();
        scalar::encode_int64(&mut vw, 2, v);
        let mut w = Writer::new();
        packed::encode_map_entry(&mut w, 9, kw.buf(), vw.buf());

        let mut r = Reader::new(w.buf());
        let _tag = r.read_tag().unwrap();
        let (rk, rv) = packed::decode_map_entry(&mut r).unwrap();
        let mut kr = Reader::new(&rk);
        prop_assert_eq!(kr.read_string_owned().unwrap(), k);
        let mut vr = Reader::new(&rv);
        prop_assert_eq!(scalar::read_int64(&mut vr).unwrap(), v);
    }
}
