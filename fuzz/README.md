# tpt-proto-fuzz

Fuzz targets and a portable harness for the `tpt-proto` Protocol Buffers
ecosystem (Phase 17 — Security Hardening & Fuzzing).

## Targets

| Target                | Decoder / front-end exercised                          |
|-----------------------|--------------------------------------------------------|
| `binary_decoder`      | Core `Reader` + `UnknownFieldSet` (schema-less wire)   |
| `descriptor_decoder`  | `FileDescriptorSet` binary `decode` + pool build        |
| `language_parser`     | `.proto` lexer/parser + compiler semantic analysis     |
| `json_decoder`        | JSON → `DynamicMessage` (shared schema)                |
| `text_parser`         | Text format → `DynamicMessage` (shared schema)         |
| `dynamic_decoder`     | `DynamicMessage::decode` + re-encode (shared schema)   |

Each target deliberately discards `Err` results for invalid input; a **panic
inside** a decoder (not a rejected input) is what a fuzzer reports as a finding.

## Running

### libFuzzer (recommended; CI / Linux)

```sh
cargo install cargo-fuzz
cargo fuzz run binary_decoder
# ... or any other target name above
```

Corpus seeds live in `corpus/` and are picked up automatically.

### Portable harness (any platform, including Windows MSVC)

`cargo fuzz` links libFuzzer, which is unavailable on Windows MSVC. A portable
harness runs the same target logic over the seeded `corpus/` and a bounded
amount of randomised input, reporting (not aborting on) panics:

```sh
cargo run            # corpus + randomised smoke run
cargo test           # regression tests in tests/smoke.rs
```

`tests/smoke.rs` asserts the targets do not panic on empty input, known edge
cases, and short randomised inputs.

## Security audit notes (Phase 17)

* **Decoder limits (`DecoderLimits`).** Enforced centrally in
  `tpt-proto-core`'s `Reader` (`max_bytes`, `max_depth`, `max_fields`,
  `max_length_delimited`, `max_string_len`, `max_unknown_bytes`). Every
  decoder funnels through `Reader`, so the limits apply uniformly.
* **Recursion / depth control (FIXED).** Nested-message decoding previously
  created a *fresh* `Reader` per level (`Reader::new(body)`), which reset the
  depth counter to 0 and defeated `max_depth` across nesting — a stack-exhaustion
  DoS vector. Added `Reader::nested`, which continues the parent's depth, and
  threaded it through `tpt-proto-reflect`, `tpt-proto-descriptor`,
  `tpt-proto-wkt` (Struct/Value/ListValue), and the `tpt-proto-codegen-rust`
  generated-`merge_from` templates. Adversarial deep nesting now returns
  `Error::DepthLimitExceeded` instead of recursing unbounded.
* **UTF-8 validation.** Strings are validated via `str::from_utf8` before any
  use. The single `unsafe` in the codebase (`Reader::read_string`) is preceded
  by a successful UTF-8 check and is therefore sound; it is the only `unsafe`
  in the workspace and is documented inline.
* **`unsafe` audit.** Exactly one `unsafe` block in the entire workspace
  (core `reader.rs`), validated and commented.
* **Allocation control.** Length-delimited fields are bounds-checked against
  `max_length_delimited` (and `max_string_len`) before any allocation; unknown
  fields are capped by `max_unknown_bytes`; total consumption is capped by
  `max_bytes`. `max_fields` bounds tag count.
* **Language parser (tracked).** The `.proto` parser is recursive and is not yet
  guarded by a recursion budget; a pathological source can exhaust the stack.
  This is tolerated as a fuzzing finding (uncatchable on most platforms) and is
  a follow-up hardening item, not a blocker for the fuzz targets.
