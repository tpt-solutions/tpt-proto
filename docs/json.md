# JSON Mapping

`tpt-proto-json` implements the protobuf JSON mapping (§12): converting between
binary protobuf messages and JSON.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## Modes

- **Canonical mode** — strict, deterministic mapping following the published
  protobuf JSON specification.
- **Relaxed mode** — accepts inputs that are not strictly canonical (e.g.
  additional leniency on number/string forms), useful for round-tripping
  loosely-formatted data.

## Field name handling

- By default, JSON object keys use **`lowerCamelCase`** derived from the proto
  field name.
- Original proto field names can be emitted/parsed instead via configuration.

## Values

- Enums may be rendered as **string names** or **numeric values**; unknown enum
  values follow a configurable policy.
- 64-bit integers (`int64`/`uint64`/`fixed64`/`sfixed64` and their signed
  variants) are emitted as JSON **strings** to avoid precision loss in
  JavaScript runtimes.
- `bytes` are emitted as **base64** strings.
- `null` in JSON is treated as the field-absent / default value.

## Default value emission

JSON output can be configured to include or omit fields whose value equals the
schema default.

## Well-known type JSON rules

Special JSON representations are implemented for the well-known types
(see [wkt](../spec.txt) §14):

| Type | JSON form |
| --- | --- |
| `Timestamp` | RFC 3339 string, e.g. `"2026-08-11T12:34:56Z"` |
| `Duration` | seconds with suffix, e.g. `"3.5s"` |
| `FieldMask` | comma-separated paths, e.g. `"user.id,user.name"` |
| `Struct` / `Value` / `ListValue` | natural JSON objects/values/arrays |
| `Any` | object with `"@type"` plus the embedded message's JSON |
| Wrapper types (`Int32Value`, …) | the wrapped scalar/null |
| `Empty` | empty object `{}` |

`Any` JSON expansion requires a type registry so the embedded message can be
decoded from its `@type`.

## Round-trips

```text
binary --(binary-to-json)--> JSON
JSON  --(json-to-binary)--> binary
```

The CLI exposes these as `binary-to-json` and `json-to-binary`
(see [Quickstart](quickstart.md) and the CLI design §18).
