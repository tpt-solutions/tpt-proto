# Implementation Decisions

This file logs major implementation decisions for tpt-proto. Entries are added
as significant choices are made, with the rationale and alternatives considered.

## Format

- Date
- Decision
- Rationale
- Alternatives considered

## Decisions

### 2024 — Workspace & crate topology
- **Decision:** Split the ecosystem into 14 focused crates (`language`, `descriptor`,
  `compiler`, `codegen-rust`, `core`, `reflect`, `json`, `text`, `wkt`,
  `conformance`, `cli`, `build`, `lint`, `grpc`) under a single Cargo workspace.
- **Rationale:** Isolated, independently-testable units map cleanly onto the
  spec sections and keep compile times and dependency surfaces small.
- **Alternatives considered:** A single monolithic crate (rejected: harder to
  test and reuse); per-feature feature-flags inside one crate (rejected: weaker
  boundary enforcement).

### 2024 — Descriptor model as the central IR
- **Decision:** The compiler lowers parsed `.proto` files into
  `FileDescriptorProto`/`FileDescriptorSet` descriptors; codegen, reflection,
  JSON, text, and CLI tools all consume descriptors rather than the raw AST.
- **Rationale:** Mirrors the protobuf ecosystem's own contracts, enables
  descriptor-driven reflection and dynamic messages, and lets generated code
  stay thin.
- **Alternatives considered:** Codegen directly from the AST (rejected: couples
  every consumer to parser internals).

### 2024 — Owned vs borrowed decode
- **Decision:** `core` provides both an owned `Message` decode path and a
  borrowed/zero-copy reader so scalar/string/bytes payloads can be referenced
  from the input buffer.
- **Rationale:** Performance-sensitive callers avoid copies; the borrowed path
  underpins benchmarks and dynamic decoding.
- **Alternatives considered:** Borrowed-only (rejected: unsafe lifetime burden
  on simple callers); owned-only (rejected: unnecessary copies on hot paths).

### 2024 — Length-delimited map values carry their field tag
- **Decision:** `packed::decode_map_entry` returns the raw, tag-inclusive bytes
  for the key (field 1) and value (field 2); generated code reads the tag and
  then the payload using the same per-type decode as ordinary fields.
- **Rationale:** Reuses the ordinary field-decode expressions, so the codegen
  for maps is uniform with the codegen for repeated/singular fields and honors
  the same presence and validation rules.
- **Alternatives considered:** Stripping the tag before returning (rejected:
  forced divergent decode paths for scalar vs message values and broke nested
  message map values).

### 2024 — Decoder limits enforced centrally
- **Decision:** A single `DecoderLimits` struct (max bytes, depth, fields,
  length-delimited, string, unknown bytes) is threaded through every decode
  path in `core`, `reflect`, `json`, `text`, and `grpc`.
- **Rationale:** One audited chokepoint for DoS resistance across all parsers.
- **Alternatives considered:** Per-crate ad-hoc limits (rejected: easy to miss
  in a new decoder).

### 2024 — gRPC over `h2` (HTTP/2)
- **Decision:** The `grpc` crate builds on the `h2` and `http` crates for
  framing/transport rather than re-implementing HTTP/2.
- **Rationale:** Correct, battle-tested HTTP/2 is essential for interop; the
  project's value is the protobuf/gRPC *semantics* layer, not a new HTTP/2 stack.
- **Alternatives considered:** Custom minimal HTTP/2 (rejected: high risk, low
  payoff); `tonic` as a dependency (rejected: would defeat the independent
  clean-room transport implementation).
