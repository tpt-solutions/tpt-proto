# Changelog

All notable changes to `tpt-proto-compiler` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the semantic-analysis and descriptor-construction layer.
- `compile` for single-file AST → `FileDescriptorProto` with diagnostics.
- `compile_set` for multi-file compilation with import/package resolution,
  cross-file duplicate detection, and editions feature resolution, returning a
  `CompileResult` plus an optional `LintReport`.
- Field-number validation (1..=536_870_911; reserved 19000..=19999).
- Duplicate symbol and duplicate field detection.
- Map expansion into synthetic nested entry messages + repeated fields.
- proto2/proto3/editions presence handling, including `proto3_optional`.
- Serialization helpers `serialize_file_descriptor` /
  `parse_file_descriptor` and `to_json_name` (lowerCamelCase derivation).
- Editions feature model in the `features` module.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
