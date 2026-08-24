# tpt-proto-cli

**Command-line interface for the tpt-proto toolchain.**

`tpt-proto-cli` exposes the whole ecosystem from the terminal: compile,
describe, generate Rust, emit descriptors, decode/encode binary, convert to/from
JSON and text format, lint schema evolution, and scaffold new projects. The
library exposes the same commands as reusable functions (see `lib.rs`).

## Commands

| Command    | What it does                                                          |
|------------|-----------------------------------------------------------------------|
| `compile`  | Parse + compile a `.proto` into a `FileDescriptorProto`.             |
| `describe` | Print a structural overview of a compiled file.                      |
| `generate` | Emit Rust source for a `.proto` (optionally with gRPC stubs).        |
| `descriptors` | Serialize the descriptor set to `FileDescriptorSet` bytes.       |
| `decode` / `encode` | Decode/encode binary wire hex for a named message.            |
| `json`     | Convert binary ↔ protobuf-JSON for a named message.                 |
| `text`     | Convert binary ↔ protobuf text format for a named message.          |
| `lint`     | Compare two `.proto` files and report breaking/warning/safe changes.|
| `diff`     | Summarize message-name differences between two descriptor-set bins.  |
| `init`     | Scaffold a minimal working `tpt-proto` starter crate.               |

## Installation

```toml
[dependencies]
tpt_proto-cli = "0.1"
```

Or build the workspace binary and run, e.g.:

```sh
cargo run -p tpt-proto-cli -- decode person.proto Person 0a05416c696365
cargo run -p tpt-proto-cli -- json person.proto Person <bytes.bin>
cargo run -p tpt-proto-cli -- lint old.proto new.proto
```

## Library use

```rust
use tpt_proto_cli::compile_set;
let set = compile_set(std::path::Path::new("ex.proto"))?;
```

## Status

Part of the in-progress `tpt-proto` workspace.

## License

Licensed under either of MIT or Apache-2.0 at your option.
