# tpt-proto-lint

**Linting and breaking-change detection for Protocol Buffers schemas.**

`tpt-proto-lint` compares an "old" and "new" schema (as
`FileDescriptorProto`s) and classifies every difference into `Safe`,
`Warning`, or `Breaking`. It powers the `tpt-proto-cli lint` command and is
useful in CI to gate schema changes.

## Classifications

- **Breaking** — e.g. package changed, message/enum/service removed, field
  number reused, field removed without reservation, field type / label /
  type-name changed, enum value removed, method input/output/streaming changed.
- **Warning** — e.g. field/enum-value/method added.
- **Safe** — e.g. message/enum/service added, field removed but declared
  reserved.

## Features

- `lint(old, new) -> Report` over messages, nested messages, enums, services,
  and methods (including streaming mode and request/response types).
- `Report::has_breaking()` to gate CI.
- `Report::render()` (human-readable) and `Report::render_json()`
  (machine-readable, one JSON object per finding).
- Each `Finding` carries a stable `code` (e.g. `FIELD_REMOVED`) for tooling.

## Installation

```toml
[dependencies]
tpt-proto-lint = "0.1"
```

## Quick start

```rust
use tpt_proto_lint::lint;
use tpt_proto_descriptor::{FileDescriptorProto, DescriptorProto, FieldDescriptorProto, FieldType, Label};

fn msg(fields: Vec<FieldDescriptorProto>) -> DescriptorProto {
    DescriptorProto { name: Some("M".into()), field: fields, ..Default::default() }
}
fn fld(name: &str, num: i32, ty: FieldType) -> FieldDescriptorProto {
    FieldDescriptorProto { name: Some(name.into()), number: Some(num), r#type: Some(ty), label: Some(Label::Optional), ..Default::default() }
}

let old = FileDescriptorProto { message_type: vec![msg(vec![fld("a", 1, FieldType::String)])], ..Default::default() };
let new = FileDescriptorProto { message_type: vec![msg(vec![])], ..Default::default() };

let report = lint(&old, &new);
assert!(report.has_breaking());
println!("{}", report.render());
```

## Status

Part of the in-progress `tpt-proto` workspace.

## License

Licensed under either of MIT or Apache-2.0 at your option.
