# tpt-proto-wkt

**Google Protocol Buffers well-known types (`google.protobuf.*`).**

`tpt-proto-wkt` provides idiomatic Rust types for the protobuf well-known
types, each implementing [`tpt_proto_core::Message`] for binary wire
compatibility, plus a `well_known_file_descriptor_set()` so the
descriptor-driven layers (reflection, JSON, text) can resolve these types by
name (e.g. for JSON `Any` expansion or `Struct`/`Value` handling).

## Provided types

| Type               | Rust type                                          |
|--------------------|----------------------------------------------------|
| `Timestamp`        | `struct { seconds: i64, nanos: i32 }`             |
| `Duration`         | `struct { seconds: i64, nanos: i32 }`             |
| `FieldMask`        | `struct { paths: Vec<String> }`                   |
| Wrappers           | `DoubleValue`, `FloatValue`, `Int64Value`, `Uint64Value`, `Int32Value`, `Uint32Value`, `BoolValue`, `StringValue`, `BytesValue` (single-field newtypes) |
| `Empty`            | unit struct                                        |
| `Struct`/`Value`/`ListValue` | dynamic JSON-like value model (`Value::Null/Number/String/Bool/Struct/List`) |
| `Any`              | `struct { type_url: String, value: Vec<u8> }`     |

All types `impl tpt_proto_core::Message`, so they round-trip through the binary
wire format and interoperate with the rest of the ecosystem.

## Installation

```toml
[dependencies]
tpt-proto-wkt = "0.1"
```

## Quick start

```rust
use tpt_proto_wkt::Timestamp;

let t = Timestamp { seconds: 1_700_000_000, nanos: 123_456_789 };
let bytes = t.encode_to_vec().unwrap();
let back = Timestamp::decode(&bytes).unwrap();
assert_eq!(t, back);
```

To make these resolvable by name in the reflection/JSON/text layers:

```rust
let set = tpt_proto_wkt::well_known_file_descriptor_set();
let pool = tpt_proto_reflect::DescriptorPool::from_file(&set.file[0]);
```

## Status

Part of the in-progress `tpt-proto` workspace.

## License

Licensed under either of MIT or Apache-2.0 at your option.
