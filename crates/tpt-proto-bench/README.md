# tpt-proto-bench

**Performance benchmarks for the tpt-proto ecosystem.**

`tpt-proto-bench` is a suite of `criterion`-style (or `std`-only) benchmarks
that exercise the hot paths across the workspace: wire encode/decode, dynamic
(JSON/text) round-trips, gRPC framing/transport, and packed/map/nested message
workloads. It is **not published** to crates.io (`publish = false`) and exists
to track performance regressions within the workspace.

## What's included

- A small dependency-free timing harness (`bench`) reporting iters, ns/iter,
  and MB/s, with warmup and `black_box` to defeat dead-code elimination.
- Reusable descriptor pools and sample-message builders: `compile_msg`,
  `build_person`, `build_big`, `build_nested`, `build_packed`, `build_maps`,
  plus `with_unknown_field` to exercise unknown-field handling.
- Embedded `.proto` sources (`PERSON_PROTO`, `BIG_PROTO`, `NESTED_PROTO`,
  `PACKED_PROTO`, `MAPS_PROTO`).
- `[[bench]]` targets: `wire`, `dynamic_json`, `grpc`, `grpc_runtime`.

## Running

```sh
cargo bench -p tpt-proto-bench
```

## Status

Part of the in-progress `tpt-proto` workspace. Internal tooling only.

## License

Licensed under either of MIT or Apache-2.0 at your option.
