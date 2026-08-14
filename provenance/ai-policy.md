# AI-Assist Policy

This document records the policy and process for AI-assisted contributions to
tpt-proto.

## Policy

- AI tooling may assist with authoring, refactoring, and reviewing code.
- All AI-generated or AI-assisted code must be reviewed and understood by a
  human contributor before it is merged.
- AI-assisted contributions must comply with the clean-room policy: no code is
  derived from or copied out of other Protocol Buffers implementations.
- Maintainers should record notable AI-assisted changes here.

## Process

1. Author uses AI tooling to draft or modify code.
2. Human reviewer verifies correctness, licensing, and clean-room compliance.
3. Reviewer records the change in this file if it is significant.

## Clean-Room Guardrails for AI Tooling

Because AI models are trained on public code, additional guardrails apply:

- When asking an AI assistant to produce protobuf/gRPC machinery, phrase
  requests in terms of the public specification (`spec.txt`) and published
  standards, never "reproduce how library X does it".
- Reviewers must confirm generated code does not reproduce the structure,
  identifiers, or test corpora of other implementations (e.g. `protobuf`,
  `prost`, `tonic`, `protobuf-rust`, `google/protobuf`).
- Generated test vectors must be independently derived; see
  `provenance/test-vectors.md`.

## Recorded Uses

- **Scaffolding & boilerplate.** AI assistance was used to draft crate
  skeletons, repeated derive/impl patterns, and documentation outlines during
  early project setup. Every result was reviewed against `spec.txt` and the
  clean-room policy before being committed.
- **Cross-cutting refactors.** AI assistance helped apply consistent changes
  (e.g. limit plumbing, doc-header trademark disclaimers) across all crates.
  Each change was verified to compile and to keep `cargo test` green.
- **Docs.** AI assistance drafted developer documentation from the in-repo
  specification; technical claims were checked against `spec.txt`.
