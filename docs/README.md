# tpt-proto Documentation

This directory contains the user and developer documentation for **tpt-proto**, a
clean-room, pure-Rust implementation of a Protocol Buffers-compatible ecosystem.

> **Trademark disclaimer.** tpt-proto is an independent clean-room
> implementation. It is not an official Protocol Buffers implementation.

## Table of contents

| Document | Description |
| --- | --- |
| [Quickstart](quickstart.md) | Get from `.proto` to running Rust code fast. |
| [Language support](language-support.md) | proto2, proto3, and editions syntax coverage. |
| [Editions support](editions.md) | Feature resolution and edition semantics. |
| [Wire format](wire-format.md) | Binary encoding, wire types, repeated/map/oneof/unknown/extensions. |
| [JSON mapping](json.md) | Binary ↔ JSON behavior, modes, and well-known types. |
| [Text format](text-format.md) | Parsing and printing the protobuf text format. |
| [Code generation](codegen.md) | What `tpt-proto-codegen-rust` emits and how presence is modeled. |
| [Reflection & dynamic messages](reflection.md) | Descriptor-driven encode/decode/inspect/mutate. |
| [Security limits](security.md) | Decoder limits, recursion, UTF-8, integer safety, unsafe policy. |
| [Deterministic encoding](deterministic-encoding.md) | Reproducible output for hashing, signing, and auditing. |
| [Conformance status](conformance.md) | Official conformance coverage and how to run it. |
| [Clean-room policy](clean-room.md) | Provenance and AI-assist policy. |
| [Licensing](licensing.md) | Dual MIT OR Apache-2.0 license, copyright, and trademark disclaimer. |
| [gRPC layer](grpc.md) | Protocol, security, observability, debugging tools, compatibility. |

## Component map

tpt-proto is a Cargo workspace of 14 crates under `crates/`:

| Crate | Responsibility |
| --- | --- |
| `tpt-proto-language` | `.proto` lexer/parser (proto2/proto3/editions). |
| `tpt-proto-descriptor` | Resolved schema model + binary descriptor (de)serialization. |
| `tpt-proto-compiler` | Import/package/semantic analysis → descriptor generation. |
| `tpt-proto-codegen-rust` | Rust code generator (structs, enums, oneofs, maps, builders). |
| `tpt-proto-core` | Binary wire-format runtime (varints, tags, codecs, limits). |
| `tpt-proto-reflect` | `DynamicMessage` descriptor-driven encoding/decoding. |
| `tpt-proto-json` | protobuf JSON mapping (canonical/relaxed, WKT rules). |
| `tpt-proto-text` | Text format print/parse. |
| `tpt-proto-wkt` | Well-known types (Timestamp, Duration, Any, Struct, …). |
| `tpt-proto-conformance` | Rust conformance testee + runner integration. |
| `tpt-proto-cli` | User-facing `tpt-proto` command set. |
| `tpt-proto-build` | `build.rs` integration. |
| `tpt-proto-lint` | Style + breaking-change detection (SAFE/WARNING/BREAKING). |
| `tpt-proto-grpc` | gRPC protocol layer over HTTP/2 (see [grpc.md](grpc.md)). |

## Repository layout

```text
tpt-proto/
  crates/        # the 14 workspace crates
  docs/          # this documentation
  examples/      # runnable examples
  tests/         # integration tests
  fuzz/          # fuzz targets
  benches/       # benchmarks
  provenance/    # clean-room provenance records
```

## Design authority

The canonical design is `spec.txt` at the repository root (main design doc §1–§30
and the gRPC addendum §1–§20). Where docs and `spec.txt` disagree, `spec.txt`
is authoritative and the docs should be corrected.
