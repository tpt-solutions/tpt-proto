# Conformance Status

`tpt-proto-conformance` provides a Rust **conformance testee** that integrates
with the official protobuf conformance test runner (§19, §4.10). Passing
conformance is a required gate for the `1.0.0` release (§27, §29.4).

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## Covered areas

The conformance suite targets the following areas:

- proto2 binary
- proto3 binary
- editions binary
- proto2 JSON
- proto3 JSON
- editions JSON
- failure behavior
- unknown field handling
- well-known type behavior

## Components

- **Rust conformance testee** — a binary implementing the conformance protocol,
  mapping test requests to `tpt-proto-core`/`tpt-proto-json` and reporting
  results and failures.
- **Official runner integration** — the testee plugs into the upstream
  conformance test driver in CI.
- **CI integration** — conformance runs automatically on supported platforms.
- **Failure reporting** — failures are reported with enough detail (test name,
  expected vs actual) to diagnose.
- **Documented exceptions** — any legal or specification issue that prevents a
  particular case is documented explicitly rather than silently skipped.

## Independent compatibility vectors

In addition to the official runner, the repository maintains **independent
compatibility vectors** derived from public specifications (§22.6). These are
not copied from any other implementation; they are constructed from the
published protobuf behavior and the project's own design (`spec.txt`).

## Running locally

Refer to the crate's instructions and the CI workflow for invoking the testee
against the conformance runner. (Exact invocation depends on the toolchain
setup in CI; see `tpt-proto-conformance` for details.)

## Status notes

Conformance coverage is tracked against §29.4 (runtime), §29.5 (JSON), and
§29.8 (well-known types). Documented exceptions, if any, live alongside the
conformance crate and are referenced here.
