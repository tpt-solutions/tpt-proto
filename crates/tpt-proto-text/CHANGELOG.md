# Changelog

All notable changes to `tpt-proto-text` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the protobuf text format printer/parser (§4.8 / §13).
- `message_to_text` emitting text format with `{ }` or `< >` delimiters,
  field names or numbers, and deterministic ordering.
- `text_to_message` supporting nested-message shorthand, maps
  (`key:`/`value:`), enums by name or number, and `[type_name] { … }`
  entries.
- `TextOptions`: `use_braces`, `use_field_numbers`, `deterministic`, and
  `max_depth` (default 100).
- `TextError` covering lexical, syntax, unknown-type, value, and
  recursion-limit failures.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
