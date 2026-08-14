//! Dynamic-decoding and JSON-mapping benchmarks (Phase 18).
//!
//! These exercise the descriptor-driven path (no generated code): building and
//! decoding [`DynamicMessage`] values, and converting them to/from JSON.

use tpt_proto_bench::{
    bench, build_big, build_person, compile_msg, BIG_PROTO, PERSON_PROTO,
};

fn main() {
    println!("=== dynamic decoding + json ===");

    // --- Dynamic decoding (already covered under wire; this isolates it). ---
    let (pool, desc) = compile_msg(PERSON_PROTO, "bench.Person");
    let person = build_person(&pool, &desc);
    let person_bytes = person.encode().unwrap();
    let pool_c = pool.clone();
    let desc_c = desc.clone();
    bench("dynamic/decode_person", person_bytes.len() as u64, 100_000, || {
        let mut r = tpt_proto_core::Reader::new(&person_bytes);
        tpt_proto_reflect::DynamicMessage::decode(&pool_c, desc_c.clone(), &mut r).unwrap()
    });

    // --- JSON: message -> JSON -> message roundtrips. ---
    use tpt_proto_json::{json_string_to_message, message_to_json_string, JsonOptions};

    let opts = JsonOptions::default();
    let json = message_to_json_string(&pool, &desc, &person, &opts).unwrap();
    let pool_c = pool.clone();
    let desc_c = desc.clone();
    bench("json/person/to_json", json.len() as u64, 50_000, || {
        message_to_json_string(&pool_c, &desc_c, &person, &opts).unwrap()
    });
    bench("json/person/from_json", json.len() as u64, 50_000, || {
        json_string_to_message(&pool_c, &desc_c, &json, &opts).unwrap()
    });
    bench("json/person/roundtrip", json.len() as u64, 30_000, || {
        let j = message_to_json_string(&pool_c, &desc_c, &person, &opts).unwrap();
        json_string_to_message(&pool_c, &desc_c, &j, &opts).unwrap()
    });

    // Large message JSON (repeated fields -> JSON arrays).
    let n = 2_000;
    let (pool, desc) = compile_msg(BIG_PROTO, "bench.Big");
    let big = build_big(&pool, &desc, n);
    let big_bytes = big.encode().unwrap();
    let opts = JsonOptions::default();
    let big_json = message_to_json_string(&pool, &desc, &big, &opts).unwrap();
    let pool_c = pool.clone();
    let desc_c = desc.clone();
    bench("json/large/to_json", big_json.len() as u64, 500, || {
        message_to_json_string(&pool_c, &desc_c, &big, &opts).unwrap()
    });
    bench("json/large/from_json", big_json.len() as u64, 200, || {
        json_string_to_message(&pool_c, &desc_c, &big_json, &opts).unwrap()
    });
    let _ = big_bytes;
}
