# tpt-proto-json

**Protocol Buffers JSON mapping (canonical §4.7 / §12).**

`tpt-proto-json` converts between descriptor-driven
[`tpt_proto_reflect::DynamicMessage`] values and `serde_json::Value`, honoring
the canonical protobuf JSON mapping rules and the well-known-type special forms
(`Timestamp`, `Duration`, `FieldMask`, `Any`, `Struct`/`Value`/`ListValue`,
wrapper types, `Empty`).

## Features

- `message_to_json` / `json_to_message` operate on a `DynamicMessage` + its
  `DescriptorPool`.
- `message_to_json_string` / `json_string_to_message` convenience wrappers.
- `JsonOptions` controls:
  - `lowerCamelCase` vs `proto_field_names`, `enum_as_ints`
  - `always_print_primitive_fields`, `int64_as_string`, `bytes_as_base64`
  - `canonical` (sorted keys, no whitespace)
  - `ignore_unknown_fields`
  - `max_depth` (default 100) — rejects runaway nesting.
- Well-known-type rendering (`Timestamp` → RFC 3339, `Duration` → seconds+nanos
  suffix, `Any` → type-url object, `Struct` → JSON object, etc.).
- `JsonError` covers unknown fields, type mismatches, unresolved types, and
  recursion limits.

## Installation

```toml
[dependencies]
tpt-proto-json = "0.1"
```

## Quick start

```rust
use tpt_proto_json::{json_string_to_message, message_to_json_string, JsonOptions};

let (pool, desc) = /* compiled descriptor + pool */;
let json = r#"{"name":"Bob","id":"42"}"#;
let dm = json_string_to_message(&pool, &desc, json, &JsonOptions::default())?;
let text = message_to_json_string(&pool, &desc, &dm, &JsonOptions::default())?;
```

## Status

Part of the in-progress `tpt-proto` workspace.

## License

Licensed under either of MIT or Apache-2.0 at your option.
