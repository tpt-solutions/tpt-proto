# Security Limits

`tpt-proto` is designed to be safe against untrusted input (§20). Malicious or
malformed messages must not cause panics, unbounded memory use, or integer
overflow. Limits are enforced consistently across the core runtime, reflection,
JSON, text, and gRPC decoders.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## Decoder limits

The runtime exposes a `DecoderLimits` struct that bounds resource consumption
during decode. All decoders honor it:

```rust,ignore
pub struct DecoderLimits {
    pub max_message_bytes: usize,        // total message size
    pub max_depth: u32,                  // nested message depth
    pub max_field_count: usize,          // number of fields
    pub max_unknown_field_bytes: usize,  // preserved unknown-field bytes
    pub max_string_bytes: usize,         // length of string fields
    pub max_bytes_field_bytes: usize,    // length of bytes fields
    pub max_repeated_entries: usize,     // entries in repeated fields
    pub max_map_entries: usize,          // entries in map fields
}
```

Limits are audited across `core`/`reflect`/`json`/`text`/`grpc` so no input path
bypasses them.

## Recursion control

Nested message depth is bounded by `max_depth`. Deeply nested or cyclic
structures are rejected rather than exhausting the stack.

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
