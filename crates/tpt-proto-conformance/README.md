# tpt-proto-conformance

**Conformance test harness and testee for tpt-proto.**

`tpt-proto-conformance` provides a Rust conformance testee that speaks the
standard framed `ConformanceRequest`/`ConformanceResponse` protocol, plus a
self-contained harness (`runner`) that exercises proto2/proto3/editions
binaries, JSON, failure behavior, unknown-field handling, and well-known-type
behavior. It is the project's compatibility gate against the protobuf
conformance suite.

## Components

- `testee` — the `ConformanceRequest`/`ConformanceResponse` implementation,
  backed by the tpt-proto wire, descriptor, reflection, JSON, and text layers.
- `runner` — `run_all` drives a set of conformance cases and produces a
  `Report` of `CaseResult`s (`Status::Pass` / `Fail` / `Skip`).
- `schema` — a `Registry` of proto schemas used by the cases.
- `protocol` — the framed request/response message types and framing.

Two binaries are provided: `tpt-conformance` (the testee) and
`tpt-conformance-testee` (alias entry point), runnable via the protobuf
conformance test runner.

## Installation

```toml
[dependencies]
tpt_proto_conformance = "0.1"
```

## Quick start (library)

```rust
use tpt_proto_conformance::runner::{run_all, Report};

let report: Report = run_all();
for case in &report.results {
    println!("{}: {:?}", case.name, case.status);
}
```

## Status

Part of the in-progress `tpt-proto` workspace. Used as the compatibility gate
in CI.

## License

Licensed under either of MIT or Apache-2.0 at your option.
