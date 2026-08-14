//! Shared helpers for the tpt-proto benchmark suite (Phase 18).
//!
//! Provides a small dependency-free timing harness (`bench`) and reusable
//! descriptor pools / sample messages so each `[[bench]]` target can focus on
//! the workload rather than boilerplate.

use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use tpt_proto_compiler::compile;
use tpt_proto_descriptor::DescriptorProto;
use tpt_proto_language::parse_file;
use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue, Value};

/// Run `f` `iters` times, measuring wall-clock time and reporting throughput.
///
/// `bytes_per_iter` is the number of *payload* bytes touched per iteration; it
/// is used to compute MB/s and to scale `iters` so slow and fast workloads both
/// run for a comparable amount of time. The closure's return value is passed
/// through [`std::hint::black_box`] to defeat dead-code elimination.
pub fn bench<R>(name: &str, bytes_per_iter: u64, iters: u64, f: impl FnMut() -> R) {
    let mut f = f;
    // Warmup so the first iteration does not dominate (code/heap caches).
    black_box(f());

    let start = Instant::now();
    for _ in 0..iters {
        black_box(f());
    }
    let elapsed = start.elapsed();

    let ns_per_iter = elapsed.as_nanos() as f64 / iters as f64;
    let total_bytes = bytes_per_iter.saturating_mul(iters);
    let mb_per_s = if ns_per_iter > 0.0 {
        (total_bytes as f64) / (ns_per_iter / 1e9) / (1024.0 * 1024.0)
    } else {
        0.0
    };
    println!(
        "{:<44} {:>9} it  {:>9.1} ns/it  {:>9.1} MB/s",
        name, iters, ns_per_iter, mb_per_s
    );
}

// ---------------------------------------------------------------------------
// Proto sources.
// ---------------------------------------------------------------------------

pub const PERSON_PROTO: &str = r#"
syntax = "proto3";
package bench;

message Sub { int32 x = 1; string y = 2; }

message Person {
  string name = 1;
  int32 id = 2;
  repeated string emails = 3;
  map<string, int32> labels = 4;
  Sub sub = 5;
}
"#;

pub const BIG_PROTO: &str = r#"
syntax = "proto3";
package bench;

message Big {
  repeated int64 nums = 1;
  repeated string strs = 2;
  repeated bytes blobs = 3;
}
"#;

pub const NESTED_PROTO: &str = r#"
syntax = "proto3";
package bench;

message Node {
  int32 val = 1;
  Node child = 2;
}
"#;

pub const PACKED_PROTO: &str = r#"
syntax = "proto3";
package bench;

message Packed {
  repeated int32 a = 1;
  repeated int64 b = 2;
  repeated double c = 3;
}
"#;

pub const MAPS_PROTO: &str = r#"
syntax = "proto3";
package bench;

message Maps {
  map<string, int32> m = 1;
  map<int32, string> m2 = 2;
}
"#;

/// Compile a single `.proto` source and return the pool plus a named message
/// descriptor. Panics on diagnostics (benchmarks must not produce diagnostics).
pub fn compile_msg(src: &str, msg: &str) -> (DescriptorPool, Arc<DescriptorProto>) {
    let parsed = parse_file("bench.proto", src);
    assert!(
        !parsed.diagnostics.has_errors(),
        "parse errors: {:?}",
        parsed.diagnostics.iter().collect::<Vec<_>>()
    );
    let (fd, diags) = compile(&parsed.file);
    assert!(
        !diags.has_errors(),
        "compile errors: {:?}",
        diags.iter().collect::<Vec<_>>()
    );
    let pool = DescriptorPool::from_file(&fd);
    let m = pool.lookup_message(msg).expect("message not found in pool");
    (pool, m)
}

// ---------------------------------------------------------------------------
// Sample message builders.
// ---------------------------------------------------------------------------

pub fn build_person(pool: &DescriptorPool, desc: &Arc<DescriptorProto>) -> DynamicMessage {
    let mut dm = DynamicMessage::new(desc.clone(), pool.clone());
    dm.set_field(1, Value::Scalar(ScalarValue::String("Alice Example".into())));
    dm.set_field(2, Value::Scalar(ScalarValue::I64(7)));
    dm.set_field(
        3,
        Value::List(vec![
            Value::Scalar(ScalarValue::String("alice@example.com".into())),
            Value::Scalar(ScalarValue::String("a@example.org".into())),
        ]),
    );
    dm.set_field(
        4,
        Value::Map(vec![
            (Value::Scalar(ScalarValue::String("home".into())), Value::Scalar(ScalarValue::I64(1))),
            (Value::Scalar(ScalarValue::String("work".into())), Value::Scalar(ScalarValue::I64(2))),
        ]),
    );
    let mut sub = DynamicMessage::new(pool.lookup_message("bench.Sub").unwrap(), pool.clone());
    sub.set_field(1, Value::Scalar(ScalarValue::I64(99)));
    sub.set_field(2, Value::Scalar(ScalarValue::String("nested".into())));
    dm.set_field(5, Value::Message(sub));
    dm
}

pub fn build_big(pool: &DescriptorPool, desc: &Arc<DescriptorProto>, n: usize) -> DynamicMessage {
    let mut dm = DynamicMessage::new(desc.clone(), pool.clone());
    let nums: Vec<Value> = (0..n as i64).map(|i| Value::Scalar(ScalarValue::I64(i))).collect();
    let strs: Vec<Value> = (0..n)
        .map(|i| Value::Scalar(ScalarValue::String(format!("str-{i}"))))
        .collect();
    let blobs: Vec<Value> = (0..n)
        .map(|i| Value::Scalar(ScalarValue::Bytes(vec![(i % 251) as u8; 16])))
        .collect();
    dm.set_field(1, Value::List(nums));
    dm.set_field(2, Value::List(strs));
    dm.set_field(3, Value::List(blobs));
    dm
}

pub fn build_nested(pool: &DescriptorPool, desc: &Arc<DescriptorProto>, depth: usize) -> DynamicMessage {
    let mut leaf = DynamicMessage::new(desc.clone(), pool.clone());
    leaf.set_field(1, Value::Scalar(ScalarValue::I64(depth as i64)));
    for _ in 0..depth {
        let mut parent = DynamicMessage::new(desc.clone(), pool.clone());
        parent.set_field(1, Value::Scalar(ScalarValue::I64(0)));
        parent.set_field(2, Value::Message(leaf));
        leaf = parent;
    }
    leaf
}

pub fn build_packed(pool: &DescriptorPool, desc: &Arc<DescriptorProto>, n: usize) -> DynamicMessage {
    let mut dm = DynamicMessage::new(desc.clone(), pool.clone());
    let a: Vec<Value> = (0..n as i32).map(|i| Value::Scalar(ScalarValue::I64(i as i64))).collect();
    let b: Vec<Value> = (0..n as i64).map(|i| Value::Scalar(ScalarValue::I64(i))).collect();
    let c: Vec<Value> = (0..n).map(|i| Value::Scalar(ScalarValue::F64((i as f64) * 0.5))).collect();
    dm.set_field(1, Value::List(a));
    dm.set_field(2, Value::List(b));
    dm.set_field(3, Value::List(c));
    dm
}

pub fn build_maps(pool: &DescriptorPool, desc: &Arc<DescriptorProto>, n: usize) -> DynamicMessage {
    let mut dm = DynamicMessage::new(desc.clone(), pool.clone());
    let m: Vec<(Value, Value)> = (0..n)
        .map(|i| {
            (
                Value::Scalar(ScalarValue::String(format!("k{i}"))),
                Value::Scalar(ScalarValue::I64(i as i64)),
            )
        })
        .collect();
    let m2: Vec<(Value, Value)> = (0..n)
        .map(|i| {
            (
                Value::Scalar(ScalarValue::I64(i as i64)),
                Value::Scalar(ScalarValue::String(format!("v{i}"))),
            )
        })
        .collect();
    dm.set_field(1, Value::Map(m));
    dm.set_field(2, Value::Map(m2));
    dm
}

/// Append an unrecognized field (number 99, varint 42) to an already-encoded
/// message, to exercise unknown-field handling.
pub fn with_unknown_field(bytes: &[u8]) -> Vec<u8> {
    use tpt_proto_core::scalar;
    use tpt_proto_core::Writer;
    let mut w = Writer::new();
    w.extend_from_slice(bytes);
    scalar::encode_int32(&mut w, 99, 42);
    w.into_vec()
}
