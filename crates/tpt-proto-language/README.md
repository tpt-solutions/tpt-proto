# tpt-proto-language

**Lexer and parser for the Protocol Buffers language (proto2 / proto3 / editions).**

`tpt-proto-language` turns `.proto` source text into a typed, lossless AST
([`ast::File`]). Diagnostic production is first-class: the parser never panics
on malformed input, instead collecting `Diagnostic`s with spans and severities
that downstream layers surface to users.

## Features

- Parses `proto2`, `proto3`, and `editions` (`edition = "2023"` and friends).
- Full surface: messages (incl. nested), enums, oneofs, maps, services/methods,
  imports (`public`/`weak`), `reserved` ranges and names, `extensions`, field
  options (`default`, `json_name`, `packed`, …), and proto2 `required`/`optional`.
- `lex` produces a token stream (`Token`/`TokenKind`) for tooling that wants to
  work below the AST.
- Rich diagnostics: `Position`, `Span`, `Severity`, `ErrorCode`.
- No I/O and no `std::error` coupling — pure transformation of `&str` to AST.

## Installation

```toml
[dependencies]
tpt-proto-language = "0.1"
```

## Quick start

```rust
use tpt_proto_language::parse_file;

let src = r#"
syntax = "proto3";
package example;

message Person {
  string name = 1;
  int32 id = 2;
  repeated string emails = 3;
}
"#;

let result = parse_file("person.proto", src);
assert!(!result.diagnostics.has_errors());
let file = result.file;
assert_eq!(file.package.as_ref().unwrap().name, "example");
assert_eq!(file.messages[0].fields.len(), 3);
```

## Crate layout

| Module        | Purpose                                                |
|---------------|--------------------------------------------------------|
| `ast`         | The typed `File` AST and all node types.              |
| `lexer`       | `lex` + `Token`/`TokenKind`, `LexError`.              |
| `parser`      | `parse_file` → `ParseResult` (file + diagnostics).    |
| `diagnostic`  | `Diagnostic`, `Diagnostics`, `ErrorCode`, `Span`, `Position`, `Severity`. |

## Status

Part of the in-progress `tpt-proto` workspace. The proto2/proto3 surface is
well covered; editions feature resolution is layered on by `tpt-proto-compiler`.

## License

Licensed under either of MIT or Apache-2.0 at your option.
