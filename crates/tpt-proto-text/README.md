# tpt-proto-text

**Protocol Buffers text format printer and parser (canonical §4.8 / §13).**

`tpt-proto-text` prints a descriptor-driven
[`tpt_proto_reflect::DynamicMessage`] to the protobuf text format and parses
text format back into a `DynamicMessage`. Useful for debugging, human-editable
configs, and conformance coverage.

## Features

- `message_to_text` — emit text format (supports `{ }` or `< >` delimiters,
  field names or numbers, and deterministic ordering).
- `text_to_message` — tokenize + parse, including nested-message shorthand
  (`field { … }`), maps (`key:`/`value:`), enums by name or number, and
  `[type_name] { … }` for `Any`-style entries.
- `TextOptions` controls `use_braces`, `use_field_numbers`, `deterministic`
  (ascending field-number order + sorted map keys), and a `max_depth` (default
  100) guard against deeply nested input.
- `TextError` covers lexical, syntax, unknown-type, value, and recursion-limit
  failures.

## Installation

```toml
[dependencies]
tpt-proto-text = "0.1"
```

## Quick start

```rust
use tpt_proto_text::{message_to_text, text_to_message, TextOptions};

let (pool, desc) = /* compiled descriptor + pool */;
let dm = /* a DynamicMessage */;
let text = message_to_text(&pool, &desc, &dm, &TextOptions::default());

let parsed = text_to_message(&pool, &desc, &text, &TextOptions::default())?;
assert_eq!(parsed.encode()?, dm.encode()?);
```

## Status

Part of the in-progress `tpt-proto` workspace.

## License

Licensed under either of MIT or Apache-2.0 at your option.
