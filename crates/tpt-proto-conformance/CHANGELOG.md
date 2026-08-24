# Changelog

All notable changes to `tpt-proto-conformance` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the conformance harness and testee (§4.10 / §19).
- `testee` implementing the framed `ConformanceRequest`/`ConformanceResponse`
  protocol over the wire/descriptor/reflection/JSON/text layers.
- `runner` (`run_all`) driving conformance cases into a `Report` of
  `CaseResult`s.
- `schema` `Registry` and `protocol` framing types.
- Binaries `tpt-conformance` and `tpt-conformance-testee`.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
