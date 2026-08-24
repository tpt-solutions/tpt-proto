# Security Limits

`tpt-proto` is designed to be safe against untrusted input (§20). Malicious or
malformed messages must not cause panics, unbounded memory use, or integer
overflow. Limits are enforced consistently across the core runtime, generated
structs, reflection, JSON, text, and gRPC decoders.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## Decoder limits

The runtime (`tpt-proto-core`) exposes `DecoderLimits`, which bounds resource
consumption during decode:

```rust
pub struct DecoderLimits {
    pub max_bytes: Option<usize>,          // total consumed across one decode
    pub max_depth: u32,                    // nested message/group depth
    pub max_fields: Option<usize>,         // tags read per decode
    pub max_length_delimited: Option<usize>,
    pub max_string_len: Option<usize>,     // UTF-8 string fields
    pub max_unknown_bytes: Option<usize>,  // preserved unknown-field bytes
}
```

Defaults are conservative (64 MiB total, depth 100, 16 MiB per value);
`DecoderLimits::unlimited()` exists only for trusted input.

Every binary decoder funnels through `Reader`, so limits apply uniformly to:
the hand-written runtime codecs, `tpt-proto-reflect` dynamic messages,
`tpt-proto-descriptor` decoding, WKT payloads, **and generated struct
decodes** — nested message fields in generated code open sub-readers with
`Reader::nested(..)`, which inherits both depth and limits instead of
resetting them. Packed elements and map entries descend through inheriting
nested readers as well, and skipping unknown legacy groups enforces
`max_depth` while it scans for the matching end-group tag.

Callers can tighten (or relax) limits for any generated type via
`tpt_proto_core::decode_with_limits::<T>(bytes, limits)`; the custom limits
propagate through every nesting level of that decode.

## Recursion control

- **Binary paths** (runtime, generated structs, reflection): bounded by
  `max_depth`; adversarial deep or cyclic input returns
  `Error::DepthLimitExceeded` rather than exhausting the stack.
- **Text format**: the parser tracks nesting and rejects input deeper than
  `TextOptions::max_depth`.
- **JSON mapping**: message/Struct/Value/ListValue conversion recursion is
  bounded by `JsonOptions::max_depth` in both directions.
  *Residual limitation:* the initial `serde_json::Value` parse itself is
  unbounded by tpt-proto; deeply nested *JSON text* is therefore capped by the
  serde_json parser's own behavior unless it is swapped out. The tpt-proto
  conversion layer on top of that tree remains depth-guarded either way.

## Allocation control

Allocations are bounded by the limits and by sanity checks on declared lengths
before any buffer is grown. A declared length that exceeds the remaining input
or the configured cap is rejected early.

## UTF-8 validation

`string` fields are validated per the schema and edition rules. proto2 and
edition-controlled strings are validated as UTF-8 on decode; behavior is
feature-resolved for editions (see [Editions support](editions.md)).

## Integer safety

All arithmetic that could overflow (length accumulation, varint decoding,
size computation) uses **checked arithmetic**.

## gRPC-layer secrets

Token/key comparison in the bundled authenticators
(`BearerTokenAuthenticator`, `MetadataAuthenticator`) uses a **constant-time**
byte comparison that does not short-circuit on the first differing byte, so
authentication timing does not leak secret contents.

## Unsafe policy

Default policy: **no `unsafe` in core decoding paths**. `unsafe` is permitted
only when it is:

- isolated,
- documented,
- tested,
- feature-gated,
- and justified by a measurable benefit.

Every `unsafe` usage is audited (currently exactly one, documented in
`fuzz/README.md`).

## Fuzzing

Fuzz targets cover the binary decoder, JSON decoder, text parser, proto language
parser, descriptor decoder, and dynamic message decoder (§22.4). These run in
CI to catch regressions in malicious-input handling.


## Allocation control

Allocations are bounded by the limits and by sanity checks on declared lengths
before any buffer is grown. A declared length that exceeds the remaining input
or the configured cap is rejected early.

## UTF-8 validation

`string` fields are validated per the schema and edition rules. proto2 and
edition-controlled strings are validated as UTF-8 on decode; behavior is
feature-resolved for editions (see [Editions support](editions.md)).

## Integer safety

All arithmetic that could overflow (length accumulation, varint decoding,
size computation) uses **checked arithmetic**.

## Unsafe policy

Default policy: **no `unsafe` in core decoding paths**. `unsafe` is permitted
only when it is:

- isolated,
- documented,
- tested,
- feature-gated,
- and justified by a measurable benefit.

Every `unsafe` usage is audited.

## Fuzzing

Fuzz targets cover the binary decoder, JSON decoder, text parser, proto language
parser, descriptor decoder, and dynamic message decoder (§22.4). These run in
CI to catch regressions in malicious-input handling.
