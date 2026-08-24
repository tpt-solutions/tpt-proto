# Lint Rules & Breaking-Change Detection

`tpt-proto-lint` (and the `tpt-proto lint` CLI command) compares an *old* and a
*new* schema and classifies every difference as `SAFE`, `WARNING`, or
`BREAKING`. Findings are available in human-readable form or as JSON
(`tpt-proto lint old.proto new.proto --json`).

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## Rule catalog

### Package & messages

| Code | Severity | Meaning |
| --- | --- | --- |
| `PACKAGE_CHANGED` | BREAKING | The proto package was renamed; every full name changes. |
| `MESSAGE_ADDED` | SAFE | A new message appeared. |
| `MESSAGE_REMOVED` | BREAKING | A message disappeared. |

### Fields

| Code | Severity | Meaning |
| --- | --- | --- |
| `FIELD_ADDED` | WARNING | A field was added — safe on the wire, but watch for oneof/JSON collisions. |
| `FIELD_REMOVED` | BREAKING | A field was removed **without** reserving its number or name. |
| `FIELD_RESERVED` | SAFE | A field was removed and properly reserved (`range`/`name`). |
| `FIELD_NUMBER_REUSED` | BREAKING | The same number now maps to a different name/type. |
| `FIELD_TYPE_CHANGED` | BREAKING | Scalar/message type changed (wire-incompatible in general). |
| `FIELD_LABEL_CHANGED` | BREAKING | optional/required/repeated changed. |
| `FIELD_TYPE_NAME_CHANGED` | BREAKING | Named type reference changed (enum/message target). |
| `JSON_NAME_CHANGED` | BREAKING | An explicit `json_name` was renamed — silently breaks JSON compatibility. Emitted as WARNING when an explicit json_name is *removed* (key becomes derived from the field name). |
| `DEFAULT_VALUE_CHANGED` | WARNING | A proto2 default value changed what unset readers observe. |

### Reserved ranges/names & extension ranges

| Code | Severity | Meaning |
| --- | --- | --- |
| `RESERVED_RANGE_SHRUNK` | BREAKING | A reserved number range narrowed or vanished, re-opening numbers to accidental reuse. |
| `RESERVED_NAME_REMOVED` | BREAKING | A reserved name was dropped from the reservation list. |
| `EXTENSION_RANGE_REMOVED` | WARNING | An extension range was removed or narrowed. |

### Enums

| Code | Severity | Meaning |
| --- | --- | --- |
| `ENUM_ADDED` | SAFE | A new enum appeared. |
| `ENUM_REMOVED` | BREAKING | An enum disappeared. |
| `ENUM_VALUE_ADDED` | WARNING | A new value was appended. |
| `ENUM_VALUE_REMOVED` | BREAKING | A value disappeared (open enums read it as unknown). |

### Services & methods

| Code | Severity | Meaning |
| --- | --- | --- |
| `SERVICE_ADDED` | SAFE | A new service appeared. |
| `SERVICE_REMOVED` | BREAKING | A service disappeared. |
| `METHOD_ADDED` | WARNING | A new RPC was added. |
| `METHOD_REMOVED` | BREAKING | An RPC disappeared. |
| `METHOD_INPUT_CHANGED` | BREAKING | The request type reference changed. |
| `METHOD_OUTPUT_CHANGED` | BREAKING | The response type reference changed. |
| `METHOD_STREAMING_CHANGED` | BREAKING | Unary ↔ streaming mode flipped. |

## Usage

```sh
# Human-readable report
tpt-proto lint proto/old/user.proto proto/new/user.proto

# Machine-readable (CI-friendly) output
tpt-proto lint proto/old/user.proto proto/new/user.proto --json
```

Exit codes: the command succeeds whenever the report is produced; CI pipelines
should fail on `has_breaking()` / any `BREAKING` tag in the JSON output.
