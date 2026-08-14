# Code Generation

`tpt-proto-codegen-rust` turns resolved descriptors (`tpt-proto-descriptor`)
into safe, idiomatic Rust (§9). It can be invoked via the `generate` CLI command
or the `tpt-proto-build` crate from `build.rs`.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## What is generated

For each message, the generator emits:

- a **struct** with one field per proto field, plus doc comments derived from
  proto comments;
- an **`encode`** method (and `encode_to_vec` helper);
- a **`decode`** method;
- a **borrowed / zero-copy `decode`** method where applicable;
- **default value** handling;
- **unknown field** storage and passthrough.

For enums it emits a Rust `enum` with named values, numeric conversion, unknown
value handling, open/closed semantics, and aliases where applicable.

Oneofs are generated as an **idiomatic Rust enum** of the member types, e.g.:

```rust
pub enum Contact {
    Email(String),
    Phone(String),
}
```

Maps use idiomatic Rust map types unless deterministic ordering requires
another representation.

## Presence modeling

Presence is modeled per the resolved schema (see
[Editions support](editions.md) and [Language support](language-support.md)):

| Schema semantics | Generated Rust |
| --- | --- |
| proto3 implicit presence | plain `T` (absent == default) |
| proto3 explicit `optional` | `Option<T>` |
| proto2 `optional` | `Option<T>` |
| proto2 `required` | `T` (must be present) |
| editions | presence determined by resolved features |

## Builders

When enabled, the generator emits **builders** that validate at construction
time:

- required fields are present;
- oneof constraints are satisfied;
- enum values are valid;
- default values are valid.

## Reflection and service hooks

Generated code includes **reflection metadata** hooks so types work with
`tpt-proto-reflect`, and **service trait** generation tying into the gRPC layer
(see [gRPC](grpc.md)). JSON and text-format support hooks are also emitted so
generated types integrate with `tpt-proto-json` and `tpt-proto-text`.

## `build.rs` integration

```rust
// build.rs
fn main() {
    tpt_proto_build::compile_protos(&["proto/user.proto"], &["proto"])
        .expect("compile protos");
}
```

Includes path, output directory, codegen options, and incremental rebuild
detection are configurable, and compile errors surface clearly to `cargo build`
output. See the `tpt-proto-build` crate.
