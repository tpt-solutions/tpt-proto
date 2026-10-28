# Security Policy

## Reporting a vulnerability

**Please do not report security vulnerabilities through public GitHub issues,
discussions, or pull requests.**

Use GitHub's **private vulnerability reporting**: open the repository on
GitHub, go to the **Security** tab, and choose
**Report a vulnerability**. This keeps the details private while we work on a
fix, and lets us coordinate disclosure with you.

Please include as much of the following as you can:

- The affected crate(s) and version(s) (e.g. `tpt-proto-core 0.x.y`).
- The component involved: binary wire-format decoding, descriptor parsing,
  JSON/text mapping, gRPC framing/transport/TLS, codegen, CLI, or build.
- A minimal reproducer (input bytes, schema, or command line) and the observed
  vs. expected behavior.
- Your assessment of impact and any known workarounds.

You can expect an initial acknowledgement within a few days and progress
updates as the fix is developed. Credit is given in the release notes unless
you prefer to remain anonymous.

## Scope notes

The highest-value targets in this project are the decoders that consume
untrusted input. `DecoderLimits` (`max_bytes`, `max_depth`, `max_fields`,
`max_length_delimited`, `max_string_len`, `max_unknown_bytes`) are enforced
centrally in `Reader::read_tag`/`advance` so every decoder funnels through
them; see [`docs/security.md`](docs/security.md) for the full decoder-limits
threat model (decompression bombs, deep-nesting stack exhaustion, oversized
allocations). The gRPC layer additionally bounds frame sizes, metadata size,
and concurrent RPCs; fuzz coverage lives under [`fuzz/`](fuzz/) with weekly
CI.

## Supported versions

Pre-1.0 support policy (from [VERSIONING.md](VERSIONING.md)): the project is
in its `0.y.z` series. Only the **latest released minor line** receives security
fixes — fixes land in the next `0.y.z` patch or `0.(y+1).0` release, since
pre-1.0 minor releases may carry breaking changes anyway. There are no LTS
branches before 1.0.

## Hardening guidance for downstream users

- Always decode untrusted input through APIs that apply `DecoderLimits`
  defaults; use `DecoderLimits::unlimited()` only for fully trusted input.
- Set explicit `max_message_size` / `max_metadata_size` on gRPC servers and
  clients rather than relying on defaults alone.
- Prefer server-side TLS via `ServerBuilder::with_tls` (feature `tls`) over
  cleartext h2c outside local development; see `docs/grpc.md` § Security.
