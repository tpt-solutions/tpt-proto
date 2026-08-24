# Changelog

All notable changes to `tpt-proto-codegen-rust` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the Rust code generator (Phase 5, §4.4 / §9).
- `generate(set, &GenerateOptions)` emitting idiomatic Rust source.
- `GenerateOptions`: `module_per_package`, `grpc`, `json`, `text`.
- Correct scalar, repeated (packed where allowed), map, oneof, and message
  codegen honoring presence semantics for proto2/proto3/editions.
- `proto3_optional` and proto2 default-value handling (custom `Default` impls).
- Embedded-descriptor support for `to_json`/`from_json` and `to_text`/
  `from_text` hooks on generated messages.
- `async_trait` server traits + client stubs under the `grpc` option.
- `CodegenError` for unsupported constructs.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
