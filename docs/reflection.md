# Reflection & Dynamic Messages

`tpt-proto-reflect` provides `DynamicMessage`: a descriptor-driven message
representation that requires **no compile-time generated types** (§11). Any
message whose `FileDescriptor`/`Descriptor` is available can be decoded,
encoded, inspected, and mutated at runtime.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## Basic usage

```rust,ignore
use tpt_proto_reflect::DynamicMessage;

// descriptor obtained from a compiled FileDescriptorSet
let message = DynamicMessage::decode(&descriptor, bytes)?;
let name = message.get_field("name")?;     // by name
let id = message.get_field_by_number(1)?;  // by number
let encoded = message.encode()?;
```

## Access and mutation

- **Field access & mutation** by name or number.
- **Repeated fields** — index/iterate/append.
- **Map fields** — keyed get/insert/remove.
- **Enum values** — get/set by name or number, including unknown values.
- **Oneofs** — inspect which member is set and read/replace it.
- **Nested messages** — recurse through `DynamicMessage` values.
- **Extensions** — resolved via the extension registry.
- **Unknown fields** — inspect and preserve/re-encode.

## Defaults and presence

The reflection API can inspect **default values** and **presence** without
generating code, honoring the resolved schema (proto2 optional/required, proto3
implicit/explicit optional, editions features).

## Registries

- A **type registry** maps message/enum names and `Any` `type_url`s to
  descriptors, enabling `Any` expansion and cross-message references.
- An **extension registry** resolves proto2 extensions by field number and
  extended type.

## Relationship to the rest of the system

- Reflection is descriptor-driven, so it shares the same model produced by
  `tpt-proto-compiler` and `tpt-proto-descriptor`.
- `tpt-proto-json` and `tpt-proto-text` operate on `DynamicMessage` for
  format conversion without generated code.
- Generated code emits reflection **metadata hooks** so the same descriptors
  back both static and dynamic usage (see [Code generation](codegen.md)).
