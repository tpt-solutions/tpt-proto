# Changelog

All notable changes to `tpt-proto-build` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the `build.rs` integration (§4.12).
- `compile_protos_simple(protos, includes)` inferring `$OUT_DIR`.
- `compile_protos(protos, includes, out_dir, &BuildConfig)` for full control.
- `BuildConfig`: `grpc`, `module_per_package`, `incremental` (fingerprint-
  based stamping to skip unchanged inputs).
- Emits `cargo:rerun-if-changed` for every input, dependency, and include
  directory.
- Generates a `mod.rs` listing every emitted module.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
