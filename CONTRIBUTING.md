# Contributing to tpt-proto

Thank you for your interest in contributing to **tpt-proto**, a clean-room,
pure-Rust Protocol Buffers-compatible ecosystem and gRPC layer.

## License

By contributing, you agree that your contributions will be dual-licensed under
the [MIT License](LICENSE-MIT) and the [Apache License, Version 2.0](LICENSE-APACHE)
("MIT OR Apache-2.0"), with copyright attributed to **TPT Solutions** unless
otherwise agreed. You must have the right to submit your contribution under
these terms.

## Clean-Room Policy

tpt-proto is an **independent clean-room implementation**. To preserve this
status:

- Do **not** copy source code from any existing Protocol Buffers
  implementation (e.g. `protobuf`, `prost`, `google/protobuf`, `tonic`,
  `protobuf-rust`, etc.).
- Do **not** paste or transcribe reference implementations, even "for
  inspiration". Re-implement functionality from the public specification
  (`spec.txt`) and published standards only.
- Test vectors must be independently derived from public specifications, not
  extracted from other projects. See `provenance/test-vectors.md`.

## AI-Assist Policy

- AI tooling may be used to assist with implementation, but all AI-generated
  code must be reviewed and understood by a human contributor before merge.
- Reviewers must confirm that AI-assisted contributions comply with the
  clean-room policy above.
- Significant AI-assisted changes should be noted in `provenance/ai-policy.md`.

## Development

- Format with `cargo fmt` and lint with `cargo clippy --all-targets`.
- Ensure `cargo test --workspace` passes before opening a pull request.
- New functionality should include tests and, where relevant, conformance
  coverage.

## Code of Conduct

Be respectful and constructive. We want tpt-proto to be a welcoming project for
contributors who care about correctness, safety, and clean implementation.
