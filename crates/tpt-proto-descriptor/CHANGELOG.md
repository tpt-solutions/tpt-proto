# Changelog

All notable changes to `tpt-proto-descriptor` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the descriptor model and binary (de)serialization.
- `FileDescriptorSet`, `FileDescriptorProto`, `DescriptorProto`,
  `FieldDescriptorProto`, `EnumDescriptorProto`, `EnumValueDescriptorProto`,
  `ServiceDescriptorProto`, and related types, each implementing
  `tpt_proto_core::Message`.
- Query helpers: `DescriptorProto::find_field_by_name` /
  `find_field_by_number` / `find_oneof_by_name` / `find_nested_message` /
  `find_nested_enum`; `EnumDescriptorProto::find_value_by_name` /
  `find_value_by_number`; `FileDescriptorProto::find_message` / `find_enum` /
  `find_service`; `FileDescriptorSet::find_file`.
- `serde` derive support for textual tooling.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
