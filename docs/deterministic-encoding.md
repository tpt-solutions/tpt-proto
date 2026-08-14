# Deterministic Encoding

`tpt-proto` provides **deterministic encoding**: a mode in which the same
logical message always produces the same byte sequence (§7.10). This is required
for hashing, signing, auditing, change detection, and any reproducible system.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## What deterministic mode controls

- **Field order** — fields are emitted in ascending field-number order.
- **Map order** — map entries are sorted by key for consistent output.
- **Unknown field order** — preserved unknown fields are emitted in a stable
  order.
- **Canonical varints** — varints use their single canonical representation
  (no non-minimal encodings).
- **Repeated field order** — where applicable, repeated fields retain their
  logical order rather than being reordered.
- **Oneof serialization** — the selected oneof member is serialized
  deterministically.

Deterministic mode is a property of the encoder configuration; it does not
change the schema or the wire format, only the ordering and canonicalization of
output.

## When to use it

Use deterministic encoding when:

- you compute a hash or signature over a message;
- you store or transmit messages that must be byte-stable for diffing;
- you need reproducible builds or reproducible test fixtures.

Normal (non-deterministic) encoding remains available and is not penalized by
the existence of the deterministic mode.

## Relationship to other formats

- The binary wire format has its own deterministic mode (see
  [Wire format](wire-format.md)).
- Text format also offers a deterministic output mode for stable snapshots
  (see [Text format](text-format.md)).
- JSON canonical mode complements deterministic binary encoding for
  JSON-based comparisons (see [JSON mapping](json.md)).
