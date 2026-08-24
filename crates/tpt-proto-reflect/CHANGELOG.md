# Changelog

All notable changes to `tpt-proto-reflect` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the descriptor-driven dynamic message layer.
- `DynamicMessage` with `get_field` / `set_field` / `clear_field`,
  oneof awareness (`which_oneof`), presence semantics (`has_field`,
  `field_has_presence`), proto2 defaults (`default_field_value`), and
  extension support.
- `ScalarValue` / `Value` dynamic value model (scalars, enums, messages,
  lists, maps).
- `DescriptorPool` type + extension registry built from a `FileDescriptorSet`
  or `FileDescriptorProto`, with `lookup_message` / `lookup_enum` /
  `get_extension` and reverse full-name lookups.
- Depth-limited decode that propagates `max_depth` across the full nesting
  chain.
- Unknown-field capture and preservation.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
