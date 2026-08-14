# Binary Wire Format

`tpt-proto-core` implements the complete Protocol Buffers binary wire format
(§7). This document describes the on-the-wire behavior. See also
[Security limits](security.md) for decoder limits and
[Deterministic encoding](deterministic-encoding.md) for reproducible output.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## Wire types

| Wire type | Meaning |
| ---:| --- |
| 0 | Varint |
| 1 | 64-bit (fixed `double`/`fixed64`/`sfixed64`) |
| 2 | Length-delimited (string/bytes/message/packed) |
| 3 | Group start (legacy) |
| 4 | Group end (legacy) |
| 5 | 32-bit (fixed `float`/`fixed32`/`sfixed32`) |

Groups (wire types 3/4) are deprecated but supported for full compatibility.

## Field encoding

Each field is encoded as a tag followed by a payload:

```text
tag = (field_number << 3) | wire_type
```

The tag is a varint. Scalar codecs cover `int32`, `int64`, `uint32`, `uint64`,
`sint32`, `sint64`, `fixed32`, `fixed64`, `sfixed32`, `sfixed64`, `bool`,
`string`, `bytes`. Signed types use zigzag encoding.

## Repeated fields

Repeated fields support:

- **unpacked** — one tag/payload pair per element;
- **packed** — a single length-delimited field holding concatenated elements;
- **mixed input** — decoders accept both packed and unpacked forms and merge
  them;
- **deterministic packed output** where applicable.

## Map fields

Maps are encoded as repeated synthetic map-entry messages. Each entry is a
message with:

```text
key   = field 1
value = field 2
```

Rules:

- duplicate entries are accepted;
- later values override earlier values by default;
- key and value types are validated;
- deterministic encoding sorts entries consistently (see
  [Deterministic encoding](deterministic-encoding.md)).

## Oneofs

- oneof fields are mutually exclusive;
- if multiple oneof fields appear in a single message, the **last value wins**;
- presence semantics follow the schema (proto2 `optional`, proto3 `optional`,
  or edition-resolved).

## Unknown fields

Default policy: **preserve** unknown fields. They are re-encodable. Alternative
policies (selectable per decode): `discard` and `fail`. Preserved unknown fields
round-trip through re-encoding.

## Extensions (proto2)

Proto2-style extensions are supported: extension ranges, extension declarations,
extension registries, dynamic extension lookup, and encode/decode of extension
fields. Option extensions are also supported.

## Legacy groups

Group start/end wire types (3/4) with matching field numbers are decoded and
encoded for compatibility. New usage with groups emits a warning.

## Message representations

The runtime supports multiple representations (§8):

- **owned** messages that own all data;
- **borrowed / zero-copy** messages that avoid allocation by borrowing from the
  input buffer;
- **bytes-backed** messages using shared/sliced buffers;
- **dynamic** messages (see [Reflection](reflection.md)).

## Integer safety

All integer arithmetic in the decoder uses checked arithmetic to prevent
overflow from malicious or malformed input.
