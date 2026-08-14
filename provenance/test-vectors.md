# Test Vectors

This document records the origin and derivation of test vectors used by
tpt-proto's test suites and conformance harness.

## Principles

- All test vectors must be independently derived from public specifications
  (the tpt-proto `spec.txt`, the published Protocol Buffers language/encoding
  guides, and RFCs referenced by well-known types).
- Test vectors must **not** be extracted from or copied out of other
  implementations' test corpora.
- Round-trip vectors are generated *in tree* by encoding known message values
  and asserting that decode reproduces them, so no external fixture is required.

## Derivation Methods

### Hand-authored message values (unit & integration tests)
- **Source:** Values written directly in Rust test code (`tests/`, `examples/`).
- **Derivation:** Constructed from the field schema documented in `spec.txt`;
  e.g. scalar/enum/map/oneof combinations chosen to exercise packed vs
  unpacked repeated fields, oneof last-value-wins, and map duplicate-key
  override. No external corpus is used.
- **Location:** `crates/*/tests/*`, `crates/tpt-proto-codegen-rust/tests/roundtrip.rs`.

### Generated round-trip vectors
- **Source:** `Message::encode_to_vec` followed by `Message::decode`, asserted
  equal. Used for scalars, nested messages, collections, maps, oneofs, enums
  (including unknown values), and unknown-field preservation.
- **Derivation:** Fully internal; the expected value is the input itself.

### Wire-format edge cases
- **Source:** Manually constructed byte slices (varint encodings, tag math,
  group start/end, packed/unpacked interleaving) in `core` and `reflect` tests.
- **Derivation:** Computed from the wire-format rules in `spec.txt` §7/§8.

### Well-known types
- **Source:** Known reference values from published RFCs/standards: RFC 3339
  timestamps, ISO-8601 durations (`"3.5s"`), RFC 4648 base64 for bytes, JSON
  `Value`/`Struct` shapes. Encoded/decoded and checked against the canonical
  forms described in `spec.txt` §14.
- **Derivation:** Independent re-derivation from the cited standards, not copied
  from another implementation's fixtures.

### Conformance suite
- **Source:** The official protobuf conformance testee protocol
  (`ConformanceRequest`/`ConformanceResponse`). tpt-proto implements the
  *testee* side and can be driven by the upstream conformance runner.
- **Derivation:** The runner supplies test cases; tpt-proto derives expected
  behavior from `spec.txt` and the conformance protocol, not from another
  implementation's outputs.

## Prohibited Sources

Test vectors must never be taken from the test directories or fixtures of other
Protocol Buffers projects (e.g. `google/protobuf`, `protobuf`, `prost`,
`tonic`, `protobuf-rust`). Any vector that cannot be traced to a public
specification or independently generated is rejected.
