//! Hand-derived wire-format compatibility vectors.
//!
//! Every byte sequence below was computed **by hand from the public Protocol
//! Buffers binary encoding rules** (tag = `field_number << 3 | wire_type`,
//! little-endian fixed values, zigzag for sint*, 10-byte sign-extended varints
//! for negative int32/enum). No reference implementation's output was consulted;
//! see `provenance/test-vectors.md`.

use std::sync::Arc;

use tpt_proto_compiler::compile;
use tpt_proto_descriptor::DescriptorProto;
use tpt_proto_language::parse_file;
use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue, Value};

const PROTO: &str = r#"
syntax = "proto3";
package vec;
message V {
  int32 a = 1;
  string s = 2;
  repeated int32 p = 3;             // packed (proto3 default)
  repeated int32 u = 4 [packed = false];
  fixed32 f32v = 5;
  fixed64 f64v = 6;
  sint32 z = 7;
  sfixed64 sf = 8;
}
"#;

fn setup() -> (DescriptorPool, Arc<DescriptorProto>) {
    let parsed = parse_file("vec.proto", PROTO);
    assert!(!parsed.diagnostics.has_errors());
    let (fd, diags) = compile(&parsed.file);
    assert!(!diags.has_errors(), "{:?}", diags.iter().collect::<Vec<_>>());
    let pool = DescriptorPool::from_file(&fd);
    let m = pool.lookup_message("vec.V").unwrap();
    (pool, m)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn decode(hexstr: &str) -> DynamicMessage {
    let (pool, desc) = setup();
    let bytes: Vec<u8> = (0..hexstr.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hexstr[i..i + 2], 16))
        .collect();
    let mut r = tpt_proto_core::Reader::new(&bytes);
    DynamicMessage::decode(&pool, desc, &mut r).unwrap()
}

#[test]
fn vector_basic_scalars_150_and_testing() {
    // a=150 -> tag 0x08 + varint(150)=96 01 ; s="testing" -> 12 07 + ASCII
    let dm = decode("089601120774657374696e67");
    assert_eq!(dm.get_field(1), Some(&Value::Scalar(ScalarValue::I64(150))));
    match dm.get_field(2) {
        Some(Value::Scalar(ScalarValue::String(s))) => assert_eq!(s, "testing"),
        other => panic!("expected string field 2, got {other:?}"),
    }
}

#[test]
fn vector_packed_vs_unpacked_repeated() {
    // p=[3,270] packed:   1a 03 03 8e 02
    // u=[1,2] unpacked:   20 01 20 02   (tag 4<<3|0 = 0x20)
    let dm = decode("1a03038e0220012002");
    match dm.get_field(3) {
        Some(Value::List(items)) => {
            let got: Vec<i64> = items
                .iter()
                .map(|v| match v {
                    Value::Scalar(ScalarValue::I64(n)) => *n,
                    other => panic!("unexpected {other:?}"),
                })
                .collect();
            assert_eq!(got, vec![3, 270]);
        }
        other => panic!("expected list field 3, got {other:?}"),
    }
    match dm.get_field(4) {
        Some(Value::List(items)) => assert_eq!(items.len(), 2),
        other => panic!("expected list field 4, got {other:?}"),
    }
}

#[test]
fn vector_negative_int32_sign_extension() {
    // a=-1 must be encoded as a full 10-byte varint: 08 + ff*9 + 01
    let dm = decode("08ffffffffffffffffff01");
    assert_eq!(dm.get_field(1), Some(&Value::Scalar(ScalarValue::I64(-1))));
}

#[test]
fn vector_fixed_and_zigzag_and_sfixed() {
    // f32v=1.0f bits? No: fixed32 is raw u32 here -> 1
    //   tag 5<<3|5=0x2d + LE 01000000
    // f64v=1: tag 6<<3|1=0x31 + LE 0100000000000000
    // z=-2 sint32: zigzag(-2)=3 -> tag 7<<3|0=0x38, 03
    // sf=-2 sfixed64: two's complement LE feffffffffffffff, tag 8<<3|1=0x41
    let bytes = "2d01000000310100000000000000380341feffffffffffffff";
    let dm = decode(bytes);
    assert_eq!(
        dm.get_field(5),
        Some(&Value::Scalar(ScalarValue::U64(1)))
    );
    assert_eq!(dm.get_field(6), Some(&Value::Scalar(ScalarValue::U64(1))));
    assert_eq!(dm.get_field(7), Some(&Value::Scalar(ScalarValue::I64(-2))));
    assert_eq!(dm.get_field(8), Some(&Value::Scalar(ScalarValue::I64(-2))));

    // Re-encoding the decoded message must reproduce these exact bytes
    // (fields are written in ascending field-number order).
    let re = dm.encode().unwrap();
    assert_eq!(hex(&re), bytes.to_lowercase());
}

#[test]
fn vector_unknown_fields_preserved_and_reordered_last() {
    // s="hi" then unknown varint field 99 value 5 (tag varint: 99<<3|0 =
    // 792 = 0x9806): 12 02 68 69 | 98 06 05
    let dm = decode("12026869980605");
    match dm.get_field(2) {
        Some(Value::Scalar(ScalarValue::String(s))) => assert_eq!(s, "hi"),
        other => panic!("expected string field 2, got {other:?}"),
    }
    assert!(!dm.unknown.is_empty(), "unknown field 99 must be preserved");
    let re = dm.encode().unwrap();
    assert_eq!(hex(&re), "12026869980605");
}

#[test]
fn vector_duplicate_map_like_entries_last_wins() {
    // Two occurrences of scalar field a=1: values 7 then 9 -> last wins (9).
    let dm = decode("08070809");
    assert_eq!(dm.get_field(1), Some(&Value::Scalar(ScalarValue::I64(9))));
}
