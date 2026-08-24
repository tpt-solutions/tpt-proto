# tpt-proto-codegen-rust

**Rust code generator for Protocol Buffers schemas.**

`tpt-proto-codegen-rust` consumes a
[`tpt_proto_descriptor::FileDescriptorSet`] (the validated output of
`tpt-proto-compiler`) and emits idiomatic Rust source implementing
[`tpt_proto_core::Message`] for every message, plus enum types, oneof enums,
map fields, and (optional) service traits/clients that the gRPC layers build
upon.

## Features

- `generate(set, &GenerateOptions)` → `String` of Rust source.
- `GenerateOptions`:
  - `module_per_package` — wrap each file's types in a package-named module.
  - `grpc` — emit `async_trait` server traits + client stubs referencing
    `tpt_proto_grpc`.
  - `json` / `text` — embed the descriptor set and emit `to_json`/`from_json`
    and `to_text`/`from_text` hooks on each message.
- Correct scalar, repeated (packed where allowed), map, oneof, and message
  codegen with presence semantics for proto2/proto3/editions.
- `proto3_optional` and proto2 defaults handled (custom `Default` impls).
- `CodegenError` for unsupported constructs.

## Installation

```toml
[dependencies]
tpt-proto-codegen-rust = "0.1"
```

## Quick start

```rust
use tpt_proto_codegen_rust::{generate, GenerateOptions};
use tpt_proto_compiler::compile_set;
use tpt_proto_language::parse_file;

let parsed = parse_file("ex.proto", "syntax=\"proto3\"; message P { string name = 1; }");
let set = compile_set("ex.proto", &parsed)?; // (helper shown for illustration)
let code = generate(&set, &GenerateOptions::default())?;
println!("{code}");
```

> In practice you'll call this from `build.rs` via `tpt-proto-build`, or from
> the CLI / `tpt-proto-cli`.

## Status

Part of the in-progress `tpt-proto` workspace. The synchronous service-trait
placeholder and the gRPC-aware stubs are produced under the `grpc` option.

## License

Licensed under either of MIT or Apache-2.0 at your option.
