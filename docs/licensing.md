# Licensing

`tpt-proto` is dual-licensed under **MIT OR Apache-2.0**, with copyright held by
**TPT Solutions**.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## License

```toml
license = "MIT OR Apache-2.0"
```

This is set across the workspace `Cargo.toml` for all 14 crates. Users may
choose either license. The repository ships both license texts:

- `LICENSE-MIT`
- `LICENSE-APACHE`

and a `COPYRIGHT` file recording the holder (**TPT Solutions**).

## Contributions

Contributions must be made under the same license (MIT OR Apache-2.0). The
contribution policy, including the clean-room and AI-assist requirements, is in
`CONTRIBUTING.md`. Contributors agree that their contributions are
dual-licensed as above.

## Trademark disclaimer

tpt-proto is an **independent clean-room implementation**. It is **not** an
official Protocol Buffers implementation, and it is not affiliated with or
endorsed by the upstream protobuf project. The project avoids confusing names,
official logos, and misleading "official" framing, and describes compatibility
carefully (per the risk policy, §28.5).

When documenting or describing tpt-proto, include the disclaimer:

```text
tpt-proto is an independent clean-room implementation.
It is not an official Protocol Buffers implementation.
```

## Versioning

All crates share a unified version and are released together under Semantic
Versioning. Pre-1.0 and post-1.0 rules are in `VERSIONING.md` at the repository
root, summarized here:

- **Pre-1.0 (`0.y.z`)** — minor bumps may carry breaking changes (including to
  wire format, generated code, or public APIs); patches are backwards
  compatible.
- **Post-1.0** — `MAJOR` for incompatible API/wire changes, `MINOR` for
  backwards-compatible additions, `PATCH` for fixes.

No `1.0.0` release will be cut until the conformance suite (Phase 16) and the
release-readiness gate (Phase 22, §29) are satisfied.
