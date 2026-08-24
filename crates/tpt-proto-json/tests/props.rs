//! Property-based tests: random valid `P` messages must survive
//! binary → JSON → binary with byte-identical re-encoding.

use std::sync::Arc;

use proptest::prelude::*;
use tpt_proto_compiler::compile;
use tpt_proto_descriptor::DescriptorProto;
use tpt_proto_json::{json_string_to_message, message_to_json_string, JsonOptions};
use tpt_proto_language::parse_file;
use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue, Value};

const PROTO: &str = r#"
syntax = "proto3";
package pr;
message P {
  string name = 1;
  int64 id = 2;
  repeated sint32 zs = 3;
}
"#;

fn setup() -> (DescriptorPool, Arc<DescriptorProto>) {
    let parsed = parse_file("pr.proto", PROTO);
    assert!(
        !parsed.diagnostics.has_errors(),
        "parse: {:?}",
        parsed.diagnostics.iter().collect::<Vec<_>>()
    );
    let (fd, diags) = compile(&parsed.file);
    assert!(
        !diags.has_errors(),
        "compile: {:?}",
        diags.iter().collect::<Vec<_>>()
    );
    let pool = DescriptorPool::from_file(&fd);
    let m = pool.lookup_message("pr.P").unwrap();
    (pool, m)
}

fn build(
    pool: &DescriptorPool,
    desc: &Arc<DescriptorProto>,
    name: &str,
    id: i64,
    zs: &[i32],
) -> DynamicMessage {
    let mut dm = DynamicMessage::new(desc.clone(), pool.clone());
    dm.set_field(1, Value::Scalar(ScalarValue::String(name.to_string())));
    dm.set_field(2, Value::Scalar(ScalarValue::I64(id)));
    let items: Vec<Value> = zs
        .iter()
        .map(|z| Value::Scalar(ScalarValue::I64(*z as i64)))
        .collect();
    dm.set_field(3, Value::List(items));
    dm
}

proptest! {
    #[test]
    fn json_roundtrip_bytes_stable(
        name in "[a-zA-Z][a-zA-Z0-9 .]{0,31}",
        id in 1i64..=i64::MAX,
        zs in prop::collection::vec(any::<i32>(), 0..8),
    ) {
        let (pool, desc) = setup();
        let opts = JsonOptions::default();

        let msg = build(&pool, &desc, &name, id, &zs);
        let bytes = msg.encode().unwrap();

        let json = message_to_json_string(&pool, &desc, &msg, &opts).unwrap();
        let back = json_string_to_message(&pool, &desc, &json, &opts).unwrap();
        let rebytes = back.encode().unwrap();
        prop_assert_eq!(rebytes, bytes);

        // JSON output must be canonical for equal messages.
        let json2 = message_to_json_string(&pool, &desc, &back, &opts).unwrap();
        prop_assert_eq!(json2, json);
    }

    /// Empty messages must round-trip to `{}` and back without loss.
    #[test]
    fn json_empty_message(_u in any::<u8>()) {
        let (pool, desc) = setup();
        let opts = JsonOptions::default();
        let msg = DynamicMessage::new(desc.clone(), pool.clone());
        let json = message_to_json_string(&pool, &desc, &msg, &opts).unwrap();
        prop_assert_eq!(json.trim(), "{}");
        let back = json_string_to_message(&pool, &desc, &json, &opts).unwrap();
        prop_assert!(back.encode().unwrap().is_empty());
    }
}

