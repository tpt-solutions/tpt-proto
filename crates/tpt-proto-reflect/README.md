# tpt-proto-reflect

**Descriptor-driven dynamic messages for Protocol Buffers.**

`tpt-proto-reflect` decodes and encodes protobuf messages purely from their
`FileDescriptorProto`, with no generated code. Everything else that is
*schema-driven but codegen-free* — JSON mapping, text format, tooling, and the
`Any`/well-known-type special forms — builds on top of `DynamicMessage`.

## Features

- `DynamicMessage` — a message value addressed by field number, with
  `get_field` / `set_field` / `clear_field`, oneof awareness (`which_oneof`),
  presence semantics (`has_field`, `field_has_presence`), proto2 defaults
  (`default_field_value`), and extension support.
- `ScalarValue` / `Value` — a dynamic value model covering scalars, enums,
  nested messages, repeated lists, and maps.
- `DescriptorPool` — a type registry (messages/enums by fully-qualified name)
  and extension registry (extensions by field number), built from a
  `FileDescriptorSet` or `FileDescriptorProto`.
- Depth-limited, limit-aware decode that propagates `max_depth` across the
  whole nesting chain (protects against stack-overflow DoS).
- Unknown-field capture and preservation.

## Installation

```toml
[dependencies]
tpt-proto-reflect = "0.1"
```

## Quick start

```rust
use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue, Value};

let (pool, desc) = /* build from a compiled .proto */;
let mut dm = DynamicMessage::new(desc.clone(), pool.clone());
dm.set_field(1, Value::Scalar(ScalarValue::String("Alice".into())));
dm.set_field(2, Value::Scalar(ScalarValue::I64(7)));

let bytes = dm.encode().unwrap();
let mut r = tpt_proto_core::Reader::new(&bytes);
let decoded = DynamicMessage::decode(&pool, desc, &mut r).unwrap();
assert_eq!(decoded.get_field(1), Some(&Value::Scalar(ScalarValue::String("Alice".into()))));
```

## Crate layout

- `DynamicMessage` and `Value`/`ScalarValue` — the dynamic model.
- `DescriptorPool` — name/extension resolution with `lookup_message`,
  `lookup_enum`, `get_extension`, and reverse full-name lookups.
- `ReflectError` — wire / unknown-field / unresolved-type / type-mismatch errors.

## Status

Part of the in-progress `tpt-proto` workspace.

## License

Licensed under either of MIT or Apache-2.0 at your option.
