# Changelog

All notable changes to `tpt-proto-lint` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the linting and breaking-change detector (§4.13 / §17).
- `lint(old, new) -> Report` over messages, nested messages, enums, services,
  and methods.
- `Severity` classification: `Safe`, `Warning`, `Breaking`, with stable
  finding `code`s (e.g. `FIELD_REMOVED`, `PACKAGE_CHANGED`, `ENUM_VALUE_ADDED`).
- `Report::has_breaking()`, `Report::render()`, `Report::render_json()`.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
