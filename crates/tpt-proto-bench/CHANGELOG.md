# Changelog

All notable changes to `tpt-proto-bench` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the performance-benchmark suite (Phase 18).
- Dependency-free timing harness `bench` reporting iters, ns/iter, and MB/s.
- Reusable descriptor pools, sample-message builders (`compile_msg`,
  `build_person`, `build_big`, `build_nested`, `build_packed`, `build_maps`),
  and `with_unknown_field`.
- Embedded `.proto` sources (`PERSON_PROTO`, `BIG_PROTO`, `NESTED_PROTO`,
  `PACKED_PROTO`, `MAPS_PROTO`).
- `[[bench]]` targets: `wire`, `dynamic_json`, `grpc`, `grpc_runtime`.

> This crate is internal (`publish = false`) and is not released to crates.io.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
