# Changelog

All notable changes to `tpt-proto-wkt` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the Google well-known types (§4.9 / §14).
- `Timestamp`, `Duration`, `FieldMask`, `Empty`.
- Wrapper types: `DoubleValue`, `FloatValue`, `Int64Value`, `Uint64Value`,
  `Int32Value`, `Uint32Value`, `BoolValue`, `StringValue`, `BytesValue`.
- `Struct` / `Value` / `ListValue` dynamic JSON-like value model.
- `Any` with `type_url` + embedded serialized message.
- All types implement `tpt_proto_core::Message`.
- `well_known_file_descriptor_set()` so reflection/JSON/text can resolve these
  types by name.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
