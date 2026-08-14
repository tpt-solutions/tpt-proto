//! Wire-format benchmarks: small/large/nested messages, repeated & packed
//! fields, maps, unknown fields, and zero-copy decoding (Phase 18).

use std::sync::Arc;

use tpt_proto_core::{scalar, Reader, WireType, Writer};
use tpt_proto_descriptor::DescriptorProto;

use tpt_proto_bench::{
    bench, build_big, build_maps, build_nested, build_packed, build_person, compile_msg,
    with_unknown_field, BIG_PROTO, MAPS_PROTO, NESTED_PROTO, PACKED_PROTO, PERSON_PROTO,
};

// --- Low-level, allocation-free manual encode/decode of a small message. ---

fn lowlevel_encode_person() -> Vec<u8> {
    let mut w = Writer::new();
    scalar::encode_string(&mut w, 1, "Alice Example");
    scalar::encode_int32(&mut w, 2, 7);
    scalar::encode_string(&mut w, 3, "alice@example.com");
    scalar::encode_string(&mut w, 3, "a@example.org");
    let mut sub = Writer::new();
    scalar::encode_int32(&mut sub, 1, 99);
    scalar::encode_string(&mut sub, 2, "nested");
    let sub_bytes = sub.into_vec();
    w.write_tag(5, WireType::LengthDelimited);
    w.write_length_delimited(&sub_bytes);
    w.into_vec()
}

/// Borrowed decode: string fields are read as `&str` (no allocation) and only
/// their length is accumulated. This is the zero-copy hot path.
fn lowlevel_decode_borrowed(bytes: &[u8]) -> usize {
    let mut r = Reader::new(bytes);
    let mut total_len = 0usize;
    while !r.is_empty() {
        let tag = r.read_tag().unwrap();
        match (tag.field_number, tag.wire_type) {
            (1, WireType::LengthDelimited) => total_len += r.read_string().unwrap().len(),
            (2, WireType::Varint) => {
                let _ = scalar::read_int32(&mut r).unwrap();
            }
            (3, WireType::LengthDelimited) => total_len += r.read_string().unwrap().len(),
            (5, WireType::LengthDelimited) => {
                let body = r.read_length_delimited().unwrap();
                let mut sr = Reader::new(body);
                while !sr.is_empty() {
                    let t = sr.read_tag().unwrap();
                    match t.field_number {
                        1 => {
                            let _ = scalar::read_int32(&mut sr).unwrap();
                        }
                        2 => total_len += sr.read_string().unwrap().len(),
                        _ => sr.skip(t.wire_type).unwrap(),
                    }
                }
            }
            _ => r.skip(tag.wire_type).unwrap(),
        }
    }
    total_len
}

fn main() {
    println!("=== wire format ===");

    // Small message (manual low-level).
    let small = lowlevel_encode_person();
    bench("wire/small/encode_lowlevel", small.len() as u64, 200_000, lowlevel_encode_person);
    bench("wire/small/decode_borrowed(zerocopy)", small.len() as u64, 200_000, || {
        lowlevel_decode_borrowed(&small)
    });

    // Small message via the descriptor-driven DynamicMessage.
    let (pool, desc) = compile_msg(PERSON_PROTO, "bench.Person");
    let person = build_person(&pool, &desc);
    let person_bytes = person.encode().unwrap();
    let pool_c = pool.clone();
    let desc_c: Arc<DescriptorProto> = desc.clone();
    bench("wire/small/encode_dynamic", person_bytes.len() as u64, 100_000, || {
        let dm = build_person(&pool_c, &desc_c);
        dm.encode().unwrap()
    });
    bench("wire/small/decode_dynamic", person_bytes.len() as u64, 100_000, || {
        let mut r = Reader::new(&person_bytes);
        tpt_proto_reflect::DynamicMessage::decode(&pool_c, desc_c.clone(), &mut r).unwrap()
    });
    bench("wire/small/roundtrip_dynamic", person_bytes.len() as u64, 100_000, || {
        let dm = build_person(&pool_c, &desc_c);
        let bytes = dm.encode().unwrap();
        let mut r = Reader::new(&bytes);
        tpt_proto_reflect::DynamicMessage::decode(&pool_c, desc_c.clone(), &mut r).unwrap()
    });

    // Large message (many repeated fields).
    let n_big = 2_000;
    let (pool, desc) = compile_msg(BIG_PROTO, "bench.Big");
    let big = build_big(&pool, &desc, n_big);
    let big_bytes = big.encode().unwrap();
    let pool_c = pool.clone();
    let desc_c = desc.clone();
    bench("wire/large/encode", big_bytes.len() as u64, 2_000, || {
        let dm = build_big(&pool_c, &desc_c, n_big);
        dm.encode().unwrap()
    });
    bench("wire/large/decode", big_bytes.len() as u64, 2_000, || {
        let mut r = Reader::new(&big_bytes);
        tpt_proto_reflect::DynamicMessage::decode(&pool_c, desc_c.clone(), &mut r).unwrap()
    });

    // Nested message (deep chain).
    let depth = 100;
    let (pool, desc) = compile_msg(NESTED_PROTO, "bench.Node");
    let nested = build_nested(&pool, &desc, depth);
    let nested_bytes = nested.encode().unwrap();
    let pool_c = pool.clone();
    let desc_c = desc.clone();
    bench("wire/nested/encode(depth=100)", nested_bytes.len() as u64, 20_000, || {
        let dm = build_nested(&pool_c, &desc_c, depth);
        dm.encode().unwrap()
    });
    bench("wire/nested/decode(depth=100)", nested_bytes.len() as u64, 20_000, || {
        let mut r = Reader::new(&nested_bytes);
        tpt_proto_reflect::DynamicMessage::decode(&pool_c, desc_c.clone(), &mut r).unwrap()
    });

    // Repeated & packed fields.
    let n_packed = 2_000;
    let (pool, desc) = compile_msg(PACKED_PROTO, "bench.Packed");
    let packed = build_packed(&pool, &desc, n_packed);
    let packed_bytes = packed.encode().unwrap();
    let pool_c = pool.clone();
    let desc_c = desc.clone();
    bench("wire/packed/encode", packed_bytes.len() as u64, 20_000, || {
        let dm = build_packed(&pool_c, &desc_c, n_packed);
        dm.encode().unwrap()
    });
    bench("wire/packed/decode", packed_bytes.len() as u64, 20_000, || {
        let mut r = Reader::new(&packed_bytes);
        tpt_proto_reflect::DynamicMessage::decode(&pool_c, desc_c.clone(), &mut r).unwrap()
    });

    // Maps.
    let n_maps = 1_000;
    let (pool, desc) = compile_msg(MAPS_PROTO, "bench.Maps");
    let maps = build_maps(&pool, &desc, n_maps);
    let maps_bytes = maps.encode().unwrap();
    let pool_c = pool.clone();
    let desc_c = desc.clone();
    bench("wire/maps/encode", maps_bytes.len() as u64, 5_000, || {
        let dm = build_maps(&pool_c, &desc_c, n_maps);
        dm.encode().unwrap()
    });
    bench("wire/maps/decode", maps_bytes.len() as u64, 5_000, || {
        let mut r = Reader::new(&maps_bytes);
        tpt_proto_reflect::DynamicMessage::decode(&pool_c, desc_c.clone(), &mut r).unwrap()
    });

    // Unknown fields: decode with an extra field then re-encode, expecting byte
    // equality (preserve policy).
    let (pool, desc) = compile_msg(PERSON_PROTO, "bench.Person");
    let person = build_person(&pool, &desc);
    let person_bytes = person.encode().unwrap();
    let with_unknown = with_unknown_field(&person_bytes);
    let pool_c = pool.clone();
    let desc_c = desc.clone();
    bench("wire/unknown/decode+reencode", with_unknown.len() as u64, 50_000, || {
        let mut r = Reader::new(&with_unknown);
        let dm = tpt_proto_reflect::DynamicMessage::decode(&pool_c, desc_c.clone(), &mut r).unwrap();
        dm.encode().unwrap()
    });
}
