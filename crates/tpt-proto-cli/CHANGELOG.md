# Changelog

All notable changes to `tpt-proto-cli` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the command-line toolchain.
- Commands: `compile`, `describe`, `generate`, `descriptors`, `decode`,
  `encode`, `json`, `text`, `lint`, `diff`, `init`.
- Reusable library functions for every command (compile, decode/encode,
  binary↔JSON, binary↔text, schema linting, project scaffolding).
- `lint_files` producing human-readable and JSON reports.
- `init_project` scaffolding a minimal working `tpt-proto` starter crate.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
