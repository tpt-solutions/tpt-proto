# Changelog

All notable changes to `tpt-proto-json` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the protobuf JSON mapping (§4.7 / §12).
- `message_to_json` / `json_to_message` on `DynamicMessage` + `DescriptorPool`.
- `message_to_json_string` / `json_string_to_message` convenience wrappers.
- `JsonOptions`: `lowerCamelCase`, `proto_field_names`, `enum_as_ints`,
  `always_print_primitive_fields`, `int64_as_string`, `bytes_as_base64`,
  `canonical`, `ignore_unknown_fields`, and `max_depth` (default 100).
- Well-known-type rendering (`Timestamp`, `Duration`, `FieldMask`, `Any`,
  `Struct`/`Value`/`ListValue`, wrappers, `Empty`).
- `JsonError` covering unknown fields, type mismatches, unresolved types, and
  recursion limits.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
