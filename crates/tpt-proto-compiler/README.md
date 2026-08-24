# tpt-proto-compiler

**Semantic analysis, import resolution, and descriptor construction.**

`tpt-proto-compiler` is the validated-AST → descriptor step of the tpt-proto
pipeline. It converts a parsed [`tpt_proto_language::ast::File`] into a
[`tpt_proto_descriptor::FileDescriptorProto`], performing the core validations
(field-number ranges, duplicates, reserved conflicts) and emitting diagnostics.
It also ships `compile_set` for multi-file compilation with import/package
resolution, duplicate detection across files, and editions feature resolution.

## Features

- `compile` — single-file AST → `FileDescriptorProto` with diagnostics.
- `compile_set` — multi-file compilation with cross-file checks and editions
  feature resolution; returns a `CompileResult` plus an optional `LintReport`.
- Field-number validation (1..=536_870_911, reserved 19000..=19999).
- Duplicate symbol and duplicate field detection.
- Map fields expand to synthetic nested entry messages + a repeated field.
- proto2/proto3/editions presence handling, including `proto3_optional`.
- Serialization helpers: `serialize_file_descriptor`, `parse_file_descriptor`,
  and `to_json_name` (lowerCamelCase derivation).

## Installation

```toml
[dependencies]
tpt-proto-compiler = "0.1"
```

## Quick start

```rust
use tpt_proto_compiler::compile;
use tpt_proto_language::parse_file;

let parsed = parse_file("ex.proto", "syntax=\"proto3\"; message P { string name = 1; }");
let (fd, diags) = compile(&parsed.file);
assert!(!diags.has_errors());

let bytes = tpt_proto_compiler::serialize_file_descriptor(&fd).unwrap();
let back  = tpt_proto_compiler::parse_file_descriptor(&bytes).unwrap();
assert_eq!(back, fd);
```

## Crate layout

| Module     | Purpose                                                       |
|------------|---------------------------------------------------------------|
| `pipeline` | `compile_set`, `CompileResult`, `LintFinding`, `LintReport`.  |
| `features` | Editions feature model: `FeatureSet`, `FeatureOverrides`, `FieldPresence`, `JsonFormat`, etc. |

## Status

Part of the in-progress `tpt-proto` workspace.

## License

Licensed under either of MIT or Apache-2.0 at your option.
