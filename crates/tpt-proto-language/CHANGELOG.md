# Changelog

All notable changes to `tpt-proto-language` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the Protocol Buffers language lexer and parser.
- Parsing of `proto2`, `proto3`, and `editions` (`edition = "..."`) syntax.
- Typed `ast::File` covering messages (incl. nested), enums, oneofs, maps,
  services/methods, imports (`public`/`weak`), `reserved` ranges/names,
  `extensions`, and field options.
- `lex` producing a `Token`/`TokenKind` stream.
- `parse_file` returning a `ParseResult` with non-fatal diagnostics.
- Diagnostic model: `Diagnostic`, `Diagnostics`, `ErrorCode`, `Span`,
  `Position`, `Severity`.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
