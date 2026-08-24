# tpt-proto

**A clean-room, pure-Rust Protocol Buffers-compatible ecosystem + gRPC layer.**

tpt-proto is a Cargo workspace of 15 crates covering the full protobuf stack:
a `.proto` parser (proto2 / proto3 / editions), semantic compiler, Rust code
generator, binary wire-format runtime, reflection/dynamic messages, JSON and
text-format mappings, well-known types, linting & breaking-change detection,
a `build.rs` integration crate, a CLI toolchain, conformance tooling, and an
HTTP/2-based gRPC server/client layer.

> **Trademark disclaimer.** tpt-proto is an independent clean-room
> implementation. It is not an official Protocol Buffers implementation.

[![CI](https://github.com/tpt-solutions/tpt-proto/actions/workflows/ci.yml/badge.svg)](https://github.com/tpt-solutions/tpt-proto/actions/workflows/ci.yml)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)
![Rust](https://img.shields.io/badge/rust-1.74%2B-orange)

## Quickstart

```toml
# Cargo.toml
[dependencies]
tpt-proto-core = "0"      # runtime: encode/decode

[build-dependencies]
tpt-proto-build = "0"     # build-time: .proto -> Rust
```

```proto
// proto/user.proto
syntax = "proto3";
package example;

message User {
  int64 id = 1;
  string name = 2;
}
```

```rust
// build.rs
fn main() {
    tpt_proto_build::compile_protos_simple(
        &["proto/user.proto".into()],
        &["proto".into()],
    )
    .expect("compile protos");
}
```

```rust
// src/main.rs
include!(concat!(env!("OUT_DIR"), "/user.rs"));

fn main() {
    let user = User { id: 42, name: "Ada".into(), ..Default::default() };
    let bytes = user.encode_to_vec().unwrap();
    let back = User::decode(&bytes).unwrap();
    assert_eq!(user, back);
}
```

Every input `.proto` and include directory gets `cargo:rerun-if-changed`
emitted automatically — schema edits rebuild without extra wiring.

## Documentation

| Document | Description |
| --- | --- |
| [Quickstart](docs/quickstart.md) | From `.proto` to running Rust code. |
| [Language support](docs/language-support.md) | proto2 / proto3 / editions coverage. |
| [Wire format](docs/wire-format.md) | Binary encoding details and limits. |
| [Code generation](docs/codegen.md) | What the generator emits; `build.rs` integration. |
| [JSON mapping](docs/json.md) · [Text format](docs/text-format.md) | Data interchange formats. |
| [Reflection](docs/reflection.md) | Dynamic messages via descriptors. |
| [Security limits](docs/security.md) | Decoder limits, recursion control, fuzzing. |
| [gRPC layer](docs/grpc.md) | Protocol, security, observability, debugging. |
| [Migrating from prost/tonic](docs/migration-from-prost.md) | Side-by-side conversion guide. |
| [Clean-room policy](docs/clean-room.md) · [Licensing](docs/licensing.md) | Provenance and legal. |

The full index lives in [`docs/README.md`](docs/README.md).

## Example

[`examples/grpc-echo`](examples/grpc-echo) is a complete, runnable crate:
`build.rs` generates types + gRPC stubs from `proto/greet.proto`, and
`src/main.rs` serves and calls them over real TCP HTTP/2:

```sh
cargo run -p grpc-echo
```

## Repository layout

```text
crates/        # the 15 workspace crates
docs/          # user & developer documentation
examples/      # runnable end-to-end examples
tests/         # cross-crate integration fixtures
fuzz/          # fuzz targets
benches/       # benchmarks
provenance/    # clean-room provenance records
```

## Contributing & licensing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the contribution and clean-room/AI-assist
policy. Dual-licensed under [MIT](LICENSE-MIT) OR
[Apache-2.0](LICENSE-APACHE); copyright TPT Solutions (see
[COPYRIGHT](COPYRIGHT)). Versioning policy: [VERSIONING.md](VERSIONING.md).
