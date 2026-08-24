# tpt-proto-build

**`build.rs` integration for compiling `.proto` files into Rust.**

`tpt-proto-build` drives the tpt-proto parser, compiler, and Rust code
generator from a crate's `build.rs`, emitting one `.rs` module per input
`.proto` into `$OUT_DIR`. It is the recommended way to use generated types in
your own crate.

## Features

- `compile_protos_simple(protos, includes)` — low-boilerplate entry point that
  infers `$OUT_DIR` from the build environment.
- `compile_protos(protos, includes, out_dir, &BuildConfig)` — full control.
- `BuildConfig`:
  - `grpc` — emit `async_trait` server traits + client stubs.
  - `module_per_package` — wrap each file's types in a package-named module.
  - `incremental` — only regenerate a `.rs` file when its `.proto` (or an
    included dependency) actually changed (fingerprint-based stamping).
- Emits `cargo:rerun-if-changed` for every input, dependency, and include
  directory, so Cargo rebuilds only when schemas change.
- Produces a generated `mod.rs` listing every module so you can
  `include!(concat!(env!("OUT_DIR"), "/mod.rs"))`.

## Installation

```toml
[build-dependencies]
tpt-proto-build = "0.1"
```

## Quick start (build.rs)

```rust,no_run
fn main() {
    tpt_proto_build::compile_protos_simple(
        &["proto/service.proto".into()],
        &["proto".into()],
    )
    .expect("protobuf compilation failed");
}
```

```rust,no_run
// src/main.rs
include!(concat!(env!("OUT_DIR"), "/mod.rs"));
```

## Status

Part of the in-progress `tpt-proto` workspace.

## License

Licensed under either of MIT or Apache-2.0 at your option.
