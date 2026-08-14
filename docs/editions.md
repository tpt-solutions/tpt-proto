# Editions Support

protobuf **editions** is the forward-looking syntax mode. Instead of choosing
between proto2- and proto3-style semantics at the `syntax` level, editions
express semantics as a set of **features** that have defaults, can be overridden
at the file/message/field/enum scope, and are inherited by nested scopes. The
compiler (`tpt-proto-compiler`) resolves all features deterministically so that
a given schema always produces the same descriptors and behavior.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## Declaring an edition

```proto
edition = "2023";

message User {
  int64 id = 1;
}
```

If no edition is declared, the schema is treated as proto2 or proto3 based on
its `syntax` declaration, and the corresponding feature defaults are applied.

## Feature categories

The compiler recognizes the following edition feature categories:

| Feature | Governs |
| --- | --- |
| Field presence | whether a scalar field tracks explicit presence (`Option<T>`) vs implicit (zero-value absent) |
| Enum type | open (unknown values allowed) vs closed (unknown values rejected) |
| Repeated field encoding | packed vs unpacked default for repeated scalars |
| UTF-8 validation | whether `string` fields are validated as UTF-8 on decode |
| Message encoding | message-level encoding details |
| JSON format behavior | how the field is rendered/parsed in JSON |

## Resolution model

1. **Defaults** are taken from the declared edition.
2. **Overrides** at a containing scope are applied to nested scopes.
3. **Local overrides** on a message/field/enum take precedence over inherited
   values.
4. The fully resolved feature set is fixed before descriptor generation,
   guaranteeing determinism (the same input always yields the same features and
   descriptors).

This means proto2-like and proto3-like behavior are simply two preset
feature-configurations, and new editions can add or adjust features without
changing the core model.

## Compatibility mapping

`tpt-proto` guarantees that the proto2-like / proto3-like semantics implied by
an edition are mapped to the same observable behavior as the equivalent legacy
`syntax`. In particular:

- a proto2-like edition produces explicit presence and open enums;
- a proto3-like edition produces implicit presence and closed enums with a
  required `0` default.

Presence and enum semantics after resolution feed directly into code generation
(see [Code generation](codegen.md)) and reflection (see
[Reflection](reflection.md)).

## Future extensibility

The feature model is open: new edition names and new feature keys can be
introduced without changing the resolution algorithm, supporting forward
compatibility with future published editions.
