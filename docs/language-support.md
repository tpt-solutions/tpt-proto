# Language Support

`tpt-proto` supports the full protobuf language surface for **proto2**,
**proto3**, and **editions**. The parser (`tpt-proto-language`) accepts all
three syntax modes, and the compiler (`tpt-proto-compiler`) resolves semantics
deterministically before generating descriptors.

See also [Editions support](editions.md) for the feature-resolution model and
the gRPC addendum §1–§8 for service syntax.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## File-level constructs

All syntax modes support:

- `syntax` / `edition` declaration
- `package` declaration
- `import`, `import public`, `import weak`
- file-level `option`s
- top-level `message`, `enum`, `service`, `extend`

```proto
syntax = "proto3";          // or: edition = "2023";
package example;
import public "google/protobuf/descriptor.proto";
```

## Message constructs

Messages support fields, nested messages/enums/extensions, `oneof`s, `map`
fields, `reserved` ranges/names, `extensions` ranges, message options, and
legacy `group` syntax.

## Field features

`name`, `number`, `type`, `label`, `default`, `json_name`, `options`, oneof
membership, repeated encoding, presence rules, retention, targets, and feature
overrides are all modeled.

## Enum constructs

Enums support values, aliases, `reserved` ranges/names, enum and value options,
and open/closed semantics. proto3 enums are closed by default; proto2 enums are
open (unknown values are allowed).

## Service constructs

Services support service options, `rpc` methods with `stream` request/response
markers, input/output types, and method options. Streaming methods feed the
gRPC layer (see [gRPC](grpc.md)).

## proto2 vs proto3 vs editions

| Aspect | proto2 | proto3 | editions |
| --- | --- | --- | --- |
| Field presence | explicit `optional`/`required` | implicit (except `optional`) | feature-resolved |
| `required`/`optional` keywords | yes | removed (except `optional`) | via features |
| Enum default | first value | first value (must be `0`) | feature-resolved |
| Extensions | yes (`extend`) | no | via features |
| Unknown fields | preserved | preserved (since 2021/3.5) | feature-resolved |
| Default values | emitted via `default` | zero values | feature-resolved |

`editions` is the forward-looking mode: instead of syntax-level toggles, it uses
**features** (field presence, enum type, repeated encoding, UTF-8 validation,
message encoding, JSON behavior) whose defaults and overrides are resolved by
the compiler. proto2-like and proto3-like behavior are expressed as editions
feature configurations. See [Editions support](editions.md).

## Diagnostics

Parse and semantic errors are reported with file, line, column, span, severity,
error code, a human-readable message, and where possible a suggested fix. See
the compiler design (§16) and [Code generation](codegen.md).
