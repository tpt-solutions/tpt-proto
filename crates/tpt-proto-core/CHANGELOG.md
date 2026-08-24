# Changelog

All notable changes to `tpt-proto-core` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the binary wire-format runtime.
- Varint and zigzag (de)coders with `MAX_VARINT_LEN`.
- `WireType` and `Tag` (field-number + wire-type) model.
- Depth- and limit-aware `Reader` (`DecoderLimits` with `max_depth`/`max_bytes`)
  that propagates limits across nested sub-readers.
- `Writer` buffer with length-delimited framing helpers.
- Per-scalar codec functions in the `scalar` module for every protobuf scalar
  type, including packed and length-delimited forms.
- Packed-field and `map`-entry helpers in the `packed` module.
- Unknown-field capture/preservation via `UnknownFieldSet` / `UnknownValue`.
- The `Message` trait (`encode` / `merge_from` / `decode` / `encode_to_vec`).

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
