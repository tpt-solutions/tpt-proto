# tpt-proto-core

**Binary wire-format runtime for Protocol Buffers — the zero-schema foundation of the tpt-proto ecosystem.**

`tpt-proto-core` provides the low-level primitives shared by every other layer
in the workspace: the parser, compiler, code generator, reflection engine,
JSON/text mappings, well-known types, and the gRPC runtime. It is
dependency-light (an optional `bytes` re-export only) and contains **no schema
knowledge** — higher layers drive it.

## Features

- Varint and zigzag (de)coders with a documented `MAX_VARINT_LEN`.
- Full wire-type and tag model (`WireType`, `Tag`).
- A depth- and limit-aware `Reader` that refuses to recurse past
  `DecoderLimits::max_depth` and enforces a total byte cap.
- A `Writer` for building length-delimited frames.
- Per-scalar encode/decode codecs (`scalar` module) covering every protobuf
  scalar type, including packed and length-delimited forms.
- Packed-field and `map`-entry helpers (`packed` module).
- Unknown-field capture and preservation (`UnknownFieldSet`, `UnknownValue`).
- A `Message` trait implemented by generated types, well-known types, and the
  dynamic message layer.

## Installation

```toml
[dependencies]
tpt-proto-core = "0.1"
```

Enable the optional `bytes` feature to re-export `bytes::Bytes` for shared /
sliced buffer support:

```toml
tpt-proto-core = { version = "0.1", features = ["bytes"] }
```

## Quick start

```rust
use tpt_proto_core::{Reader, Writer, Message};

// Any type implementing `Message` can round-trip through the wire format.
let mut buf = Writer::new();
buf.encode_string(1, "hello");
buf.encode_int32(2, 42);
let bytes = buf.into_vec();

let mut r = Reader::new(&bytes);
let s = r.read_string_owned().unwrap();   // field 1
let n = tpt_proto_core::scalar::read_int32(&mut r).unwrap(); // field 2
assert_eq!((s, n), ("hello".to_string(), 42));
```

## Crate layout

| Module        | Purpose                                                        |
|---------------|----------------------------------------------------------------|
| `varint`      | `encode_varint` / `decode_varint`, zigzag helpers, `MAX_VARINT_LEN`. |
| `wire_type`   | `WireType` enum and the `Tag` (field number + wire type) type. |
| `reader`      | `Reader` — depth/limit-aware decode stream with sub-readers.   |
| `writer`      | `Writer` — growable encode buffer.                             |
| `scalar`      | Per-scalar codec functions (`read_*`/`encode_*` for all types). |
| `packed`      | Packed repeated encoders/decoders and `map`-entry framing.    |
| `message`     | The `Message` trait, `decode_with_limits`, limit config.      |
| `unknown`     | `UnknownFieldSet` / `UnknownValue` storage.                   |
| `limits`      | `DecoderLimits` (max depth, max bytes).                       |
| `error`       | `Error` / `Result` types.                                     |

## Status

Part of the in-progress `tpt-proto` workspace (see `../README.md`). The wire
format matches the protobuf binary encoding spec; scalar, packed, and map
encodings are covered by property tests.

## License

Licensed under either of MIT or Apache-2.0 at your option.
