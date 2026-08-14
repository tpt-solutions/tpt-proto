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

### Built-in harness (no external dependencies)

The `tpt-proto-conformance` crate ships a self-contained harness that exercises
the full tpt-proto stack end-to-end and requires nothing beyond a Rust toolchain:

```sh
cargo run -p tpt-proto-conformance -- run          # human-readable report
cargo run -p tpt-proto-conformance -- run --json   # machine-readable report
cargo run -p tpt-proto-conformance -- cases        # list generated case names
```

The harness exits non-zero if any case fails, so it is CI-friendly.

### Official `conformance_test_runner` integration

The standalone `tpt-conformance-testee` binary speaks *only* the standard framed
`ConformanceRequest`/`ConformanceResponse` protocol on stdin/stdout, with no
subcommand, so the reference protobuf runner can drive it directly:

```sh
cargo build --release --bin tpt-conformance-testee
conformance/run_conformance.sh            # locates conformance_test_runner on PATH
# or explicitly:
conformance_test_runner --enforce_recommended \
    --failure_list conformance/failure_list.txt \
    target/release/tpt-conformance-testee
```

Official suite message-type names (e.g.
`protobuf_test_messages.proto3.TestAllTypesProto3`) are aliased to tpt-proto's
dialect descriptors inside the testee (`schema.rs`), so genuine binary + JSON
cases run instead of being skipped. Cases the testee cannot run (e.g. JSPB or
text-format output) are reported back as `skipped`, which the reference runner
excludes from the pass/fail tally. Known divergences can be listed in
`conformance/failure_list.txt`.

## Status notes

Conformance coverage is tracked against §29.4 (runtime), §29.5 (JSON), and
§29.8 (well-known types). Documented exceptions, if any, live alongside the
conformance crate and are referenced here.
