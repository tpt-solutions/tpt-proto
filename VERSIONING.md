# Versioning Policy

tpt-proto follows [Semantic Versioning 2.0.0](https://semver.org/), with the
following pre-1.0 rules until a `1.0.0` release is cut.

## Pre-1.0 (current: `0.y.z`)

- The major version is `0`. Until `1.0.0`, the project treats **minor**
  version bumps (`0.y.z` → `0.(y+1).0`) as carrying breaking changes, and
  **patch** version bumps (`0.y.z` → `0.y.(z+1)`) as backwards-compatible
  fixes and additions.
- Breaking changes (including to the wire format, generated code, or public
  APIs) may occur in any `0.y+1.0` release.

## Post-1.0 (`1.0.0` and later)

- `MAJOR` increments on incompatible API or wire-format changes.
- `MINOR` increments on backwards-compatible functionality additions.
- `PATCH` increments on backwards-compatible bug fixes.

## Crates

All 14 workspace crates share a unified version (`workspace.package.version`)
and are released together. A breaking change in any public crate surface
triggers a major-version bump for the whole workspace.

## Conformance

No `1.0.0` release will be made until the conformance suite (Phase 16) and the
release-readiness gate (Phase 22, §29) are satisfied.
