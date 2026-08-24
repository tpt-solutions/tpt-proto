# tpt-proto-descriptor

**Descriptor model and binary (de)serialization for Protocol Buffers.**

`tpt-proto-descriptor` models the `google.protobuf.*` descriptor messages
(`FileDescriptorSet`, `FileDescriptorProto`, `DescriptorProto`,
`FieldDescriptorProto`, `EnumDescriptorProto`, `ServiceDescriptorProto`, …) and
provides their binary (de)serialization plus ergonomic query helpers. It is the
contract between the compiler, the code generator, and the reflection / JSON /
text engines.

## Features

- Complete descriptor message types, each implementing `tpt_proto_core::Message`
  for lossless binary round-trips.
- Structural query helpers on the descriptor types:
  - `DescriptorProto::find_field_by_name` / `find_field_by_number`
  - `DescriptorProto::find_oneof_by_name` / `find_nested_message` / `find_nested_enum`
  - `EnumDescriptorProto::find_value_by_name` / `find_value_by_number`
  - `FileDescriptorProto::find_message` / `find_enum` / `find_service`
  - `FileDescriptorSet::find_file`
- `serde` derive support for tooling that wants a textual form.

## Installation

```toml
[dependencies]
tpt-proto-descriptor = "0.1"
```

## Quick start

```rust
use tpt_proto_compiler::compile;       // produces a FileDescriptorProto
use tpt_proto_language::parse_file;

let parsed = parse_file("ex.proto", "syntax=\"proto3\"; message M { int32 x = 1; }");
let (fd, _diags) = compile(&parsed.file);

let m = fd.find_message("M").expect("message present");
let f = m.find_field_by_name("x").expect("field present");
assert_eq!(f.number, Some(1));
```

## Crate layout

- `types` (private): the descriptor message structs and their `Message` impls.
- Re-exports the full descriptor surface plus the query helpers above.

## Status

Part of the in-progress `tpt-proto` workspace. Descriptor binary compatibility
with `protoc` is validated by the conformance harness (`tpt-proto-conformance`).

## License

Licensed under either of MIT or Apache-2.0 at your option.
