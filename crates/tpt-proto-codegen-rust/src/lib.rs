//! `tpt-proto-codegen-rust` — Rust source code generation for protobuf schemas.
//!
//! Phase 5 (§4.4, §9). Consumes a [`tpt_proto_descriptor::FileDescriptorSet`]
//! (the validated output of `tpt-proto-compiler`) and emits idiomatic Rust
//! source implementing [`tpt_proto_core::Message`] for every message, plus
//! enum types, oneof enums, map fields, and (synchronous placeholder) service
//! traits that the gRPC layers build upon.
//!
//! Generated code is itself produced by [`generate`]; the crate also exposes
//! the lower-level helpers used to build that code.

use std::collections::{HashMap, HashSet};

use tpt_proto_descriptor::{
    DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FieldType, FileDescriptorSet,
    Label, OneofDescriptorProto,
};

/// An error encountered while generating code.
#[derive(Debug, thiserror::Error)]
pub enum CodegenError {
    /// A schema construct could not be represented in generated Rust.
    #[error("unsupported construct in code generation: {0}")]
    Unsupported(String),
}

/// Options controlling code generation.
#[derive(Debug, Clone, Default)]
pub struct GenerateOptions {
    /// Emit a module named after the package around each file's types.
    ///
    /// When disabled (default), all types are emitted into a single flat
    /// namespace (type names are globally unique via their fully-qualified
    /// proto name), which keeps cross-reference paths simple.
    pub module_per_package: bool,
    /// Emit async gRPC server traits and client stubs for `service` blocks,
    /// referencing the `tpt_proto_grpc` runtime types.
    pub grpc: bool,
}

// ---------------------------------------------------------------------------
// Name helpers.
// ---------------------------------------------------------------------------

/// Convert a fully-qualified proto name (e.g. `.example.Person.Address`) into a
/// globally-unique Rust type name (`ExamplePersonAddress`).
fn to_rust_type_name(fqn: &str) -> String {
    let fqn = fqn.strip_prefix('.').unwrap_or(fqn);
    let mut out = String::new();
    for seg in fqn.split('.') {
        out.push_str(&to_pascal(seg));
    }
    out
}

/// Convert an identifier to `snake_case`.
fn to_snake(s: &str) -> String {
    let mut out = String::new();
    let mut prev_lower = false;
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i != 0 && prev_lower {
                out.push('_');
            }
            out.extend(c.to_lowercase());
            prev_lower = false;
        } else {
            out.push(c);
            prev_lower = c.is_lowercase() || c.is_numeric();
        }
    }
    out
}

/// Convert an identifier to `PascalCase`.
fn to_pascal(s: &str) -> String {
    let mut out = String::new();
    let mut upper = true;
    for c in s.chars() {
        if c == '_' || c == '-' {
            upper = true;
            continue;
        }
        if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Type classification.
// ---------------------------------------------------------------------------

/// A reference to a resolved field value type.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TyRef {
    Scalar(FieldType),
    Message(String),
    Enum(String),
}

impl TyRef {
    fn from_field(f: &FieldDescriptorProto, type_map: &HashMap<String, String>) -> Option<TyRef> {
        let ty = f.r#type?;
        match ty {
            FieldType::Message => {
                let fqn = f.type_name.clone()?;
                if type_map.contains_key(&fqn) {
                    Some(TyRef::Message(fqn))
                } else {
                    Some(TyRef::Message(fqn))
                }
            }
            FieldType::Enum => {
                let fqn = f.type_name.clone()?;
                Some(TyRef::Enum(fqn))
            }
            other => Some(TyRef::Scalar(other)),
        }
    }

    fn rust(&self, type_map: &HashMap<String, String>) -> String {
        match self {
            TyRef::Scalar(t) => scalar_rust(*t).to_string(),
            TyRef::Message(fqn) | TyRef::Enum(fqn) => type_map
                .get(fqn)
                .cloned()
                .unwrap_or_else(|| to_rust_type_name(fqn)),
        }
    }
}

/// Encode a scalar type's Rust representation.
fn scalar_rust(t: FieldType) -> &'static str {
    match t {
        FieldType::Double => "f64",
        FieldType::Float => "f32",
        FieldType::Int64 => "i64",
        FieldType::Uint64 => "u64",
        FieldType::Int32 => "i32",
        FieldType::Fixed64 => "u64",
        FieldType::Fixed32 => "u32",
        FieldType::Bool => "bool",
        FieldType::String => "String",
        FieldType::Group => "Vec<u8>",
        FieldType::Message => "Vec<u8>",
        FieldType::Bytes => "Vec<u8>",
        FieldType::Uint32 => "u32",
        FieldType::Enum => "i32",
        FieldType::Sfixed32 => "i32",
        FieldType::Sfixed64 => "i64",
        FieldType::Sint32 => "i32",
        FieldType::Sint64 => "i64",
    }
}

/// Read a scalar value of `t` from reader expression `r` (a `&mut Reader`).
///
/// Handles every scalar type, including `string` (owned) and `bytes`, so the
/// same helper can service map keys, repeated fields, and singular fields.
/// The reader `r` is passed by auto-ref (it is already a `&mut Reader`).
fn dec_scalar_expr(t: FieldType, r: &str) -> String {
    match t {
        FieldType::Int32 => format!("scalar::read_int32({r})?"),
        FieldType::Int64 => format!("scalar::read_int64({r})?"),
        FieldType::Uint32 => format!("scalar::read_uint32({r})?"),
        FieldType::Uint64 => format!("scalar::read_uint64({r})?"),
        FieldType::Bool => format!("scalar::read_bool({r})?"),
        FieldType::Sint32 => format!("scalar::read_sint32({r})?"),
        FieldType::Sint64 => format!("scalar::read_sint64({r})?"),
        FieldType::Fixed32 => format!("scalar::read_fixed32({r})?"),
        FieldType::Sfixed32 => format!("scalar::read_sfixed32({r})?"),
        FieldType::Fixed64 => format!("scalar::read_fixed64({r})?"),
        FieldType::Sfixed64 => format!("scalar::read_sfixed64({r})?"),
        FieldType::Float => format!("scalar::read_float({r})?"),
        FieldType::Double => format!("scalar::read_double({r})?"),
        FieldType::String => format!("{r}.read_string_owned()?"),
        FieldType::Bytes => format!("{r}.read_length_delimited()?.to_vec()"),
        // Message/group/enum are not scalars and use dedicated paths.
        FieldType::Message | FieldType::Group | FieldType::Enum => format!("unreachable!()"),
    }
}

/// Like [`dec_scalar_expr`] but without a trailing `?`, for use inside closures
/// that themselves return `Result` (e.g. `packed::read_packed_varint`'s reader).
fn dec_scalar_expr_nq(t: FieldType, r: &str) -> String {
    match t {
        FieldType::Int32 => format!("scalar::read_int32({r})"),
        FieldType::Int64 => format!("scalar::read_int64({r})"),
        FieldType::Uint32 => format!("scalar::read_uint32({r})"),
        FieldType::Uint64 => format!("scalar::read_uint64({r})"),
        FieldType::Bool => format!("scalar::read_bool({r})"),
        FieldType::Sint32 => format!("scalar::read_sint32({r})"),
        FieldType::Sint64 => format!("scalar::read_sint64({r})"),
        FieldType::Fixed32 => format!("scalar::read_fixed32({r})"),
        FieldType::Sfixed32 => format!("scalar::read_sfixed32({r})"),
        FieldType::Fixed64 => format!("scalar::read_fixed64({r})"),
        FieldType::Sfixed64 => format!("scalar::read_sfixed64({r})"),
        FieldType::Float => format!("scalar::read_float({r})"),
        FieldType::Double => format!("scalar::read_double({r})"),
        FieldType::String => format!("{r}.read_string_owned()"),
        FieldType::Bytes => format!("{r}.read_length_delimited()?.to_vec()"),
        FieldType::Message | FieldType::Group | FieldType::Enum => format!("unreachable!()"),
    }
}

/// The `scalar::encode_*` function name for a scalar type.
fn enc_scalar_fn(t: FieldType) -> &'static str {
    match t {
        FieldType::Int32 => "encode_int32",
        FieldType::Int64 => "encode_int64",
        FieldType::Uint32 => "encode_uint32",
        FieldType::Uint64 => "encode_uint64",
        FieldType::Bool => "encode_bool",
        FieldType::Sint32 => "encode_sint32",
        FieldType::Sint64 => "encode_sint64",
        FieldType::Fixed32 => "encode_fixed32",
        FieldType::Sfixed32 => "encode_sfixed32",
        FieldType::Fixed64 => "encode_fixed64",
        FieldType::Sfixed64 => "encode_sfixed64",
        FieldType::Float => "encode_float",
        FieldType::Double => "encode_double",
        FieldType::String => "encode_string",
        FieldType::Bytes => "encode_bytes",
        FieldType::Enum => "encode_enum",
        _ => "encode_int32",
    }
}

/// A value expression that packs a scalar `v` into a `u64` for packed encoding.
fn pack_to(t: FieldType, v: &str) -> String {
    match t {
        FieldType::Int32
        | FieldType::Int64
        | FieldType::Uint32
        | FieldType::Uint64
        | FieldType::Bool => format!("{v} as u64"),
        FieldType::Enum => format!("{v}.as_i32() as u64"),
        FieldType::Sint32 => format!("__core::encode_zigzag32({v}) as u64"),
        FieldType::Sint64 => format!("__core::encode_zigzag64({v})"),
        FieldType::Fixed32 | FieldType::Sfixed32 => format!("{v} as u32"),
        FieldType::Fixed64 | FieldType::Sfixed64 => format!("{v} as u64"),
        _ => format!("{v} as u64"),
    }
}

/// Reconstruct a scalar value from a packed-decoded `raw` (u64 or u32).
fn pack_from(t: FieldType, raw: &str) -> String {
    match t {
        FieldType::Int32 => format!("{raw} as i32"),
        FieldType::Int64 => format!("{raw} as i64"),
        FieldType::Uint32 => format!("{raw} as u32"),
        FieldType::Uint64 => format!("{raw}"),
        FieldType::Bool => format!("{raw} != 0"),
        FieldType::Enum => format!("{raw} as i32"),
        FieldType::Sint32 => format!("__core::scalar::decode_zigzag32({raw} as u32)"),
        FieldType::Sint64 => format!("__core::scalar::decode_zigzag64({raw})"),
        FieldType::Fixed32 => format!("{raw}"),
        FieldType::Sfixed32 => format!("{raw} as i32"),
        FieldType::Fixed64 => format!("{raw}"),
        FieldType::Sfixed64 => format!("{raw} as i64"),
        FieldType::Float => format!("f32::from_bits({raw})"),
        FieldType::Double => format!("f64::from_bits({raw})"),
        _ => format!("{raw}"),
    }
}

/// Whether a scalar type is a fixed-width 32-bit value.
fn is_fixed32(t: FieldType) -> bool {
    matches!(
        t,
        FieldType::Fixed32 | FieldType::Sfixed32 | FieldType::Float
    )
}

/// Whether a scalar type is a fixed-width 64-bit value.
fn is_fixed64(t: FieldType) -> bool {
    matches!(
        t,
        FieldType::Fixed64 | FieldType::Sfixed64 | FieldType::Double
    )
}

// ---------------------------------------------------------------------------
// Schema collection.
// ---------------------------------------------------------------------------

struct Schema {
    type_map: HashMap<String, String>,
    skip: HashSet<String>,
    map_entries: HashMap<String, (TyRef, TyRef)>,
}

fn is_map_entry(options: &Option<Vec<u8>>) -> bool {
    // `message MessageOptions { bool map_entry = 7; }` encodes as tag 0x38, 0x01.
    options.as_deref() == Some(&[0x38u8, 0x01u8][..])
}

impl Schema {
    fn build(set: &FileDescriptorSet) -> Schema {
        let mut type_map = HashMap::new();
        let mut skip = HashSet::new();
        let mut map_entries = HashMap::new();

        for file in &set.file {
            let pkg = file.package.clone().unwrap_or_default();
            for m in &file.message_type {
                collect_message(&pkg, &[], m, &mut type_map, &mut skip, &mut map_entries);
            }
            for e in &file.enum_type {
                let name = e.name.clone().unwrap_or_default();
                let full = format!(".{}", [pkg.as_str(), name.as_str()].join("."));
                register_keys(&mut type_map, &pkg, &[], &name, to_rust_type_name(&full));
            }
        }

        Schema {
            type_map,
            skip,
            map_entries,
        }
    }
}

fn qualify(prefix: &str, name: &str) -> String {
    let prefix = prefix.strip_prefix('.').unwrap_or(prefix);
    if prefix.is_empty() {
        format!(".{name}")
    } else {
        format!(".{prefix}.{name}")
    }
}

/// Register a type under every ancestor-path-qualified key the compiler might
/// emit for a cross-type reference (e.g. `.pkg.Name`, `.pkg.Parent.Name`), all
/// mapping to the same canonical Rust type name.
fn register_keys(
    type_map: &mut HashMap<String, String>,
    pkg: &str,
    ancestors: &[String],
    name: &str,
    rust: String,
) {
    for l in 0..=ancestors.len() {
        let mut parts = vec![pkg.to_string()];
        for a in &ancestors[..l] {
            parts.push(a.clone());
        }
        parts.push(name.to_string());
        type_map.insert(format!(".{}", parts.join(".")), rust.clone());
    }
    type_map.entry(name.to_string()).or_insert(rust);
}

#[allow(clippy::too_many_arguments)]
fn collect_message(
    pkg: &str,
    ancestors: &[String],
    m: &DescriptorProto,
    type_map: &mut HashMap<String, String>,
    skip: &mut HashSet<String>,
    map_entries: &mut HashMap<String, (TyRef, TyRef)>,
) {
    let name = m.name.clone().unwrap_or_default();
    let mut parts = vec![pkg.to_string()];
    if !ancestors.is_empty() {
        parts.push(ancestors.join("."));
    }
    parts.push(name.clone());
    let full = format!(".{}", parts.join("."));
    let rust = to_rust_type_name(&full);
    register_keys(type_map, pkg, ancestors, &name, rust.clone());

    if is_map_entry(&m.options) {
        skip.insert(full.clone());
        if m.field.len() >= 2 {
            let k = field_tyref(&m.field[0]);
            let v = field_tyref(&m.field[1]);
            if let (Some(k), Some(v)) = (k, v) {
                map_entries.insert(full.clone(), (k, v));
            }
        }
    }

    let mut child_ancestors = ancestors.to_vec();
    child_ancestors.push(name.clone());
    for n in &m.nested_type {
        collect_message(pkg, &child_ancestors, n, type_map, skip, map_entries);
    }
    for e in &m.enum_type {
        let ename = e.name.clone().unwrap_or_default();
        let mut eparts = vec![pkg.to_string()];
        if !child_ancestors.is_empty() {
            eparts.push(child_ancestors.join("."));
        }
        eparts.push(ename.clone());
        let efull = format!(".{}", eparts.join("."));
        register_keys(
            type_map,
            pkg,
            &child_ancestors,
            &ename,
            to_rust_type_name(&efull),
        );
    }
}

fn field_tyref(f: &FieldDescriptorProto) -> Option<TyRef> {
    let ty = f.r#type?;
    match ty {
        FieldType::Message | FieldType::Group => f.type_name.clone().map(TyRef::Message),
        FieldType::Enum => f.type_name.clone().map(TyRef::Enum),
        other => Some(TyRef::Scalar(other)),
    }
}

// ---------------------------------------------------------------------------
// Code generation entry point.
// ---------------------------------------------------------------------------

/// Generate Rust source code for an entire [`FileDescriptorSet`].
pub fn generate(
    set: &FileDescriptorSet,
    options: &GenerateOptions,
) -> Result<String, CodegenError> {
    let schema = Schema::build(set);
    let mut out = String::new();

    out.push_str(
        "// @generated by tpt-proto-codegen-rust. DO NOT EDIT.\n\
         use tpt_proto_core as __core;\n\
         use __core::{Message, Reader, Writer, UnknownFieldSet, WireType};\n\
         use __core::scalar;\n\
         use __core::packed;\n\
         use std::collections::HashMap;\n\n",
    );

    if options.grpc {
        out.push_str(
            "use tpt_proto_grpc as __grpc;\n\
             use async_trait::async_trait;\n\
             use __grpc::{Channel, ClientStream, ServerStream};\n\n",
        );
    }

    // Enums first so messages can reference them. Nested enums are emitted
    // inline by `gen_message` as it recurses, so they stay forward-visible.
    for file in &set.file {
        let pkg = file.package.clone().unwrap_or_default();
        for e in &file.enum_type {
            out.push_str(&gen_enum(
                &qualify(&pkg, &e.name.clone().unwrap_or_default()),
                e,
            ));
        }
    }

    // Messages.
    for file in &set.file {
        let pkg = file.package.clone().unwrap_or_default();
        let syntax = file.syntax.clone().unwrap_or_else(|| "proto2".to_string());
        for m in &file.message_type {
            gen_message(&pkg, m, &schema, &mut out, &syntax);
        }
    }

    // Services: async gRPC server traits + client stubs when enabled, else a
    // synchronous placeholder trait.
    for file in &set.file {
        let pkg = file.package.clone().unwrap_or_default();
        for s in &file.service {
            out.push_str(&gen_service(s, &pkg, options.grpc));
        }
    }

    Ok(out)
}

// (Nested enums are emitted inline by `gen_message`.)

fn gen_enum(fqn: &str, e: &EnumDescriptorProto) -> String {
    let rust = to_rust_type_name(fqn);
    let mut s = String::new();
    s.push_str(&format!(
        "/// Generated from protobuf enum `{}`.\n",
        fqn.trim_start_matches('.')
    ));
    s.push_str(&format!(
        "#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]\n"
    ));
    s.push_str("#[repr(i32)]\n");
    s.push_str(&format!("pub enum {rust} {{\n"));
    let mut first = true;
    for v in &e.value {
        let vname = to_pascal(v.name.as_deref().unwrap_or("Unknown"));
        let num = v.number.unwrap_or(0);
        if first {
            s.push_str(&format!("    #[default]\n"));
            first = false;
        }
        s.push_str(&format!("    {vname} = {num},\n"));
    }
    s.push_str(&format!("    Unknown(i32),\n"));
    s.push_str("}\n\n");

    s.push_str(&format!("impl {rust} {{\n"));
    s.push_str("    /// The numeric wire value of this enum variant.\n");
    s.push_str("    pub fn as_i32(self) -> i32 {\n");
    s.push_str("        match self {\n");
    for v in &e.value {
        let vname = to_pascal(v.name.as_deref().unwrap_or("Unknown"));
        let num = v.number.unwrap_or(0);
        s.push_str(&format!("            {rust}::{vname} => {num},\n"));
    }
    s.push_str(&format!("            {rust}::Unknown(v) => v,\n"));
    s.push_str("        }\n");
    s.push_str("    }\n\n");
    s.push_str("    /// Build a variant from a numeric value, preserving unknown values.\n");
    s.push_str("    pub fn from_i32(v: i32) -> Self {\n");
    s.push_str("        match v {\n");
    for v in &e.value {
        let vname = to_pascal(v.name.as_deref().unwrap_or("Unknown"));
        let num = v.number.unwrap_or(0);
        s.push_str(&format!("            {num} => {rust}::{vname},\n"));
    }
    s.push_str(&format!("            other => {rust}::Unknown(other),\n"));
    s.push_str("        }\n");
    s.push_str("    }\n");
    s.push_str("}\n\n");
    s
}

// ---------------------------------------------------------------------------
// Message generation.
// ---------------------------------------------------------------------------

struct MessageContext<'a> {
    rust: String,
    syntax: String,
    schema: &'a Schema,
}

fn gen_message(prefix: &str, m: &DescriptorProto, schema: &Schema, out: &mut String, syntax: &str) {
    let fqn = qualify(prefix, &m.name.clone().unwrap_or_default());
    if schema.skip.contains(&fqn) {
        for n in &m.nested_type {
            gen_message(&fqn, n, schema, out, syntax);
        }
        for e in &m.enum_type {
            let efqn = qualify(&fqn, &e.name.clone().unwrap_or_default());
            out.push_str(&gen_enum(&efqn, e));
        }
        return;
    }

    let ctx = MessageContext {
        rust: to_rust_type_name(&fqn),
        syntax: normalize_syntax(syntax),
        schema,
    };
    let has_defaults = m.field.iter().any(|f| f.default_value.is_some());

    // Oneof enums.
    for (oi, o) in m.oneof_decl.iter().enumerate() {
        out.push_str(&gen_oneof_enum(&ctx, m, oi, o));
    }

    // Struct definition.
    let mut struct_def = String::new();
    struct_def.push_str(&format!(
        "/// Generated from protobuf message `{}`.\n",
        fqn.trim_start_matches('.')
    ));
    let mut derives = vec!["Debug", "Clone", "PartialEq"];
    if !has_defaults {
        derives.push("Default");
    }
    struct_def.push_str(&format!("#[derive({})]\n", derives.join(", ")));
    struct_def.push_str(&format!("pub struct {} {{\n", ctx.rust));
    for f in &m.field {
        if f.oneof_index.is_some() || f.extendee.is_some() {
            continue;
        }
        let snake = to_snake(f.name.as_deref().unwrap_or("field"));
        if is_map_field(f, schema) {
            let (k, v) = map_kv(f, schema);
            let kt = k.rust(&schema.type_map);
            let vt = v.rust(&schema.type_map);
            struct_def.push_str(&format!("    pub {snake}: HashMap<{kt}, {vt}>,\n"));
        } else {
            let tyref = match TyRef::from_field(f, &schema.type_map) {
                Some(t) => t,
                None => continue,
            };
            let rust_ty = match presence(&ctx, f) {
                Presence::Repeated => format!("Vec<{}>", tyref.rust(&schema.type_map)),
                Presence::Explicit => format!("Option<{}>", tyref.rust(&schema.type_map)),
                _ => tyref.rust(&schema.type_map),
            };
            struct_def.push_str(&format!("    pub {snake}: {rust_ty},\n"));
        }
    }
    for o in &m.oneof_decl {
        let snake = to_snake(o.name.as_deref().unwrap_or("oneof"));
        let oneof_rust = oneof_enum_name(&ctx, o);
        struct_def.push_str(&format!("    pub {snake}: Option<{oneof_rust}>,\n"));
    }
    struct_def.push_str("    pub unknown_fields: UnknownFieldSet,\n");
    struct_def.push_str("}\n\n");
    out.push_str(&struct_def);

    // Reflection metadata hook (full name of the message).
    out.push_str(&format!(
        "impl {} {{\n    /// Protobuf full name of this message.\n    pub const PROTO_FULL_NAME: &'static str = \"{}\";\n}}\n\n",
        ctx.rust,
        fqn.trim_start_matches('.')
    ));

    // Custom Default impl when proto2 defaults are present.
    if has_defaults {
        out.push_str(&gen_default_impl(&ctx, m));
    }

    // Message impl with encode/decode.
    out.push_str(&gen_message_impl(&ctx, m));

    // Builder.
    out.push_str(&gen_builder(&ctx, m));

    // Convenience encode/decode methods (zero-copy decode from a borrowed slice).
    out.push_str(&gen_convenience_methods(&ctx));

    // Nested messages/enums.
    for n in &m.nested_type {
        gen_message(&fqn, n, schema, out, syntax);
    }
    for e in &m.enum_type {
        let efqn = qualify(&fqn, &e.name.clone().unwrap_or_default());
        out.push_str(&gen_enum(&efqn, e));
    }
}

/// Normalize the `syntax` string into an internal presence/packing mode.
///
/// `proto3` uses implicit presence; `proto2` uses explicit presence (Option).
/// Editions (and any unknown syntax) default to explicit presence so that
/// proto2-like schemas and editions 2024 do not lose field presence. Per-field
/// `proto3_optional` is honored separately.
fn normalize_syntax(syntax: &str) -> String {
    match syntax {
        "proto3" => "proto3".to_string(),
        _ => "proto2".to_string(),
    }
}

// (field-type computation is inlined into `gen_message` via `presence`.)

fn is_map_field(f: &FieldDescriptorProto, schema: &Schema) -> bool {
    f.label == Some(Label::Repeated)
        && f.r#type == Some(FieldType::Message)
        && f.type_name
            .as_ref()
            .map_or(false, |t| schema.map_entries.contains_key(t))
}

fn map_kv(f: &FieldDescriptorProto, schema: &Schema) -> (TyRef, TyRef) {
    let entry = f.type_name.as_ref().and_then(|t| schema.map_entries.get(t));
    match entry {
        Some((k, v)) => (k.clone(), v.clone()),
        None => (
            TyRef::Scalar(FieldType::String),
            TyRef::Scalar(FieldType::Int32),
        ),
    }
}

fn oneof_enum_name(ctx: &MessageContext<'_>, o: &OneofDescriptorProto) -> String {
    format!(
        "{}_{}",
        ctx.rust,
        to_pascal(o.name.as_deref().unwrap_or("oneof"))
    )
}

fn gen_oneof_enum(
    ctx: &MessageContext<'_>,
    m: &DescriptorProto,
    oi: usize,
    o: &OneofDescriptorProto,
) -> String {
    let name = oneof_enum_name(ctx, o);
    let mut s = String::new();
    s.push_str("#[derive(Debug, Clone, PartialEq)]\n");
    s.push_str(&format!("pub enum {name} {{\n"));
    for f in m.field.iter().filter(|f| f.oneof_index == Some(oi as i32)) {
        let vname = to_pascal(f.name.as_deref().unwrap_or("field"));
        let tyref = match TyRef::from_field(f, &ctx.schema.type_map) {
            Some(t) => t,
            None => continue,
        };
        let vt = oneof_variant_type(&ctx, f, &tyref);
        s.push_str(&format!("    {vname}({vt}),\n"));
    }
    s.push_str("}\n\n");
    s
}

fn oneof_variant_type(ctx: &MessageContext<'_>, f: &FieldDescriptorProto, tyref: &TyRef) -> String {
    let _ = f;
    tyref.rust(&ctx.schema.type_map)
}

fn gen_message_impl(ctx: &MessageContext<'_>, m: &DescriptorProto) -> String {
    let mut s = String::new();
    s.push_str(&format!("impl Message for {} {{\n", ctx.rust));

    // encode.
    s.push_str("    fn encode(&self, w: &mut Writer) -> __core::Result<()> {\n");
    let mut fields: Vec<&FieldDescriptorProto> = m.field.iter().collect();
    fields.sort_by_key(|f| f.number.unwrap_or(0));
    for f in &fields {
        if f.extendee.is_some() || f.oneof_index.is_some() {
            continue;
        }
        let num = f.number.unwrap_or(0) as u32;
        let snake = to_snake(f.name.as_deref().unwrap_or("field"));
        if is_map_field(f, ctx.schema) {
            s.push_str(&gen_map_encode(ctx, f, num, &snake));
            continue;
        }
        let tyref = match TyRef::from_field(f, &ctx.schema.type_map) {
            Some(t) => t,
            None => continue,
        };
        s.push_str(&gen_field_encode(ctx, f, &tyref, num, &snake));
    }
    for (oi, o) in m.oneof_decl.iter().enumerate() {
        s.push_str(&gen_oneof_encode(ctx, m, oi, o));
    }
    s.push_str("        self.unknown_fields.encode(w);\n");
    s.push_str("        Ok(())\n");
    s.push_str("    }\n\n");

    // merge_from.
    s.push_str(
        "    fn merge_from(&mut self, r: &mut Reader) -> __core::Result<()> {\n\
                while !r.is_empty() {\n\
                    let tag = r.read_tag()?;\n\
                    match tag.field_number {\n",
    );
    for f in &fields {
        if f.extendee.is_some() || f.oneof_index.is_some() {
            continue;
        }
        let num = f.number.unwrap_or(0) as u32;
        let snake = to_snake(f.name.as_deref().unwrap_or("field"));
        if is_map_field(f, ctx.schema) {
            s.push_str(&format!("{num} => {{\n"));
            s.push_str(&gen_map_decode(ctx, f, &snake));
            s.push_str("}\n");
            continue;
        }
        let tyref = match TyRef::from_field(f, &ctx.schema.type_map) {
            Some(t) => t,
            None => continue,
        };
        s.push_str(&format!("{num} => {{\n"));
        s.push_str(&gen_field_decode(ctx, f, &tyref, &snake));
        s.push_str("}\n");
    }
    for (oi, o) in m.oneof_decl.iter().enumerate() {
        s.push_str(&gen_oneof_decode(ctx, m, oi, o));
    }
    s.push_str(
        "                _ => {\n\
                            self.unknown_fields.store(tag, r)?;\n\
                        }\n\
                    }\n\
                }\n\
                Ok(())\n\
            }\n",
    );
    s.push_str("}\n\n");
    s
}

fn presence(ctx: &MessageContext<'_>, f: &FieldDescriptorProto) -> Presence {
    match f.label {
        Some(Label::Repeated) => Presence::Repeated,
        Some(Label::Required) => Presence::Explicit,
        Some(Label::Optional) => {
            if f.proto3_optional == Some(true) {
                Presence::Explicit
            } else if ctx.syntax == "proto3" {
                Presence::Implicit
            } else {
                Presence::Explicit
            }
        }
        None => Presence::Implicit,
    }
}

enum Presence {
    Implicit,
    Explicit,
    Repeated,
}

fn gen_field_encode(
    ctx: &MessageContext<'_>,
    f: &FieldDescriptorProto,
    tyref: &TyRef,
    num: u32,
    snake: &str,
) -> String {
    let pres = presence(ctx, f);
    match pres {
        Presence::Implicit => {
            let guard = implicit_guard(tyref, ctx, snake);
            let mut s = String::new();
            if let Some(g) = guard {
                s.push_str(&format!("        if {g} {{\n"));
                s.push_str(&format!(
                    "            {}\n",
                    enc_value("w", num, tyref, format!("&self.{snake}"))
                ));
                s.push_str("        }\n");
            } else {
                s.push_str(&format!(
                    "        {}\n",
                    enc_value("w", num, tyref, format!("&self.{snake}"))
                ));
            }
            s
        }
        Presence::Explicit => match tyref {
            TyRef::Scalar(_) | TyRef::Enum(_) => format!(
                "        if let Some(v) = &self.{snake} {{\n            {}\n        }}\n",
                enc_value("w", num, tyref, "v".to_string())
            ),
            TyRef::Message(_) => format!(
                "        if let Some(v) = &self.{snake} {{\n            scalar::encode_message(w, {num}, &v.encode_to_vec().unwrap());\n        }}\n",
            ),
        },
        Presence::Repeated => gen_repeated_encode(tyref, num, snake, ctx),
    }
}

fn implicit_guard(tyref: &TyRef, ctx: &MessageContext<'_>, snake: &str) -> Option<String> {
    match tyref {
        TyRef::Scalar(FieldType::String) | TyRef::Scalar(FieldType::Bytes) => {
            Some(format!("!self.{snake}.is_empty()"))
        }
        TyRef::Scalar(FieldType::Bool) => Some(format!("self.{snake}")),
        TyRef::Scalar(FieldType::Float) | TyRef::Scalar(FieldType::Double) => {
            Some(format!("self.{snake} != 0.0"))
        }
        TyRef::Scalar(_) => Some(format!("self.{snake} != 0")),
        TyRef::Enum(_) => {
            let e = tyref.rust(&ctx.schema.type_map);
            Some(format!("self.{snake} != {e}::default()"))
        }
        TyRef::Message(m) => {
            let t = ctx
                .schema
                .type_map
                .get(m)
                .cloned()
                .unwrap_or_else(|| to_rust_type_name(m));
            Some(format!("self.{snake} != {t}::default()"))
        }
    }
}

/// Emit an encode statement for a *value* held in `val_expr` (a Rust
/// expression, e.g. `self.x` or `v`) at field `num`.
fn enc_value(buf: &str, num: u32, tyref: &TyRef, val_expr: String) -> String {
    match tyref {
        TyRef::Scalar(t) => {
            match t {
                FieldType::String => format!("scalar::encode_string({buf}, {num}, &{val_expr});"),
                FieldType::Bytes => format!("scalar::encode_bytes({buf}, {num}, &{val_expr});"),
                FieldType::Enum => {
                    format!("scalar::encode_enum({buf}, {num}, ({val_expr}).as_i32());")
                }
                FieldType::Message | FieldType::Group => {
                    format!("scalar::encode_message({buf}, {num}, &{val_expr}.encode_to_vec().unwrap());")
                }
                other => format!(
                    "scalar::{}({buf}, {num}, *({val_expr}));",
                    enc_scalar_fn(*other)
                ),
            }
        }
        TyRef::Enum(_) => format!("scalar::encode_enum({buf}, {num}, ({val_expr}).as_i32());"),
        TyRef::Message(_) => {
            format!("scalar::encode_message({buf}, {num}, &{val_expr}.encode_to_vec().unwrap());")
        }
    }
}

fn gen_repeated_encode(tyref: &TyRef, num: u32, snake: &str, ctx: &MessageContext<'_>) -> String {
    let target = format!("self.{snake}");
    match tyref {
        TyRef::Message(_) => format!(
            "        for v in &{target} {{\n            scalar::encode_message(w, {num}, &v.encode_to_vec().unwrap());\n        }}\n"
        ),
        TyRef::Scalar(FieldType::String) => format!(
            "        for v in &{target} {{\n            scalar::encode_string(w, {num}, v);\n        }}\n"
        ),
        TyRef::Scalar(FieldType::Bytes) => format!(
            "        for v in &{target} {{\n            scalar::encode_bytes(w, {num}, v);\n        }}\n"
        ),
        TyRef::Enum(_) => {
            let packed = ctx.syntax != "proto2";
            if packed {
                format!(
                    "        if !{target}.is_empty() {{\n            packed::encode_packed_varint(w, {num}, {target}.iter().map(|&v| v.as_i32() as u64));\n        }}\n"
                )
            } else {
                format!(
                    "        for &v in &{target} {{\n            scalar::encode_enum(w, {num}, v.as_i32());\n        }}\n"
                )
            }
        }
        TyRef::Scalar(t) if is_fixed32(*t) => {
            let packed = ctx.syntax != "proto2";
            if packed {
                format!(
                    "        if !{target}.is_empty() {{\n            packed::encode_packed_fixed32(w, {num}, {target}.iter().map(|&v| v as u32));\n        }}\n"
                )
            } else {
                format!(
                    "        for &v in &{target} {{\n            scalar::{}(w, {num}, v);\n        }}\n",
                    enc_scalar_fn(*t)
                )
            }
        }
        TyRef::Scalar(t) if is_fixed64(*t) => {
            let packed = ctx.syntax != "proto2";
            if packed {
                format!(
                    "        if !{target}.is_empty() {{\n            packed::encode_packed_fixed64(w, {num}, {target}.iter().map(|&v| v as u64));\n        }}\n"
                )
            } else {
                format!(
                    "        for &v in &{target} {{\n            scalar::{}(w, {num}, v);\n        }}\n",
                    enc_scalar_fn(*t)
                )
            }
        }
        TyRef::Scalar(t) if is_varint(*t) => {
            let packed = ctx.syntax != "proto2";
            let map_expr = pack_to(*t, "v");
            if packed {
                format!(
                    "        if !{target}.is_empty() {{\n            packed::encode_packed_varint(w, {num}, {target}.iter().map(|&v| {map_expr}));\n        }}\n"
                )
            } else {
                format!(
                    "        for &v in &{target} {{\n            scalar::{}(w, {num}, v);\n        }}\n",
                    enc_scalar_fn(*t)
                )
            }
        }
        _ => String::new(),
    }
}

fn is_varint(t: FieldType) -> bool {
    !is_fixed32(t)
        && !is_fixed64(t)
        && !matches!(
            t,
            FieldType::String | FieldType::Bytes | FieldType::Message | FieldType::Group
        )
}

fn gen_field_decode(
    ctx: &MessageContext<'_>,
    f: &FieldDescriptorProto,
    tyref: &TyRef,
    snake: &str,
) -> String {
    let pres = presence(ctx, f);
    match pres {
        Presence::Implicit => decode_assign(&format!("self.{snake}"), tyref, "r", false),
        Presence::Explicit => decode_assign(&format!("self.{snake}"), tyref, "r", true),
        Presence::Repeated => gen_repeated_decode(tyref, snake, ctx),
    }
}

/// Build a decode assignment into `target` from reader `r`.
fn decode_assign(target: &str, tyref: &TyRef, r: &str, is_option: bool) -> String {
    match tyref {
        TyRef::Message(m) => {
            let t = m.clone();
            if is_option {
                format!(
                    "            let body = {r}.read_length_delimited()?;\n            {target}.get_or_insert_with({}::default).merge_from(&mut {r}.nested(body)?)?;\n",
                    to_rust_type_name(&t)
                )
            } else {
                format!(
                    "            {target} = {}::default();\n            {target}.merge_from(&mut {r}.nested({r}.read_length_delimited()?)?)?;\n",
                    to_rust_type_name(&t)
                )
            }
        }
        TyRef::Scalar(FieldType::String) => {
            if is_option {
                format!("            {target} = Some({r}.read_string_owned()?);\n")
            } else {
                format!("            {target} = {r}.read_string_owned()?;\n")
            }
        }
        TyRef::Scalar(FieldType::Bytes) => {
            if is_option {
                format!("            {target} = Some({r}.read_length_delimited()?.to_vec());\n")
            } else {
                format!("            {target} = {r}.read_length_delimited()?.to_vec();\n")
            }
        }
        TyRef::Scalar(t) => {
            let expr = dec_scalar_expr(*t, r);
            if is_option {
                format!("            {target} = Some({expr});\n")
            } else {
                format!("            {target} = {expr};\n")
            }
        }
        TyRef::Enum(e) => {
            let en = to_rust_type_name(e);
            if is_option {
                format!("            {target} = Some({en}::from_i32(scalar::read_int32({r})?));\n")
            } else {
                format!("            {target} = {en}::from_i32(scalar::read_int32({r})?);\n")
            }
        }
    }
}

fn gen_repeated_decode(tyref: &TyRef, snake: &str, ctx: &MessageContext<'_>) -> String {
    let target = format!("self.{snake}");
    match tyref {
        TyRef::Message(m) => {
            let t = to_rust_type_name(m);
            format!(
                "            let body = r.read_length_delimited()?;\n            let mut mv = {t}::default();\n            mv.merge_from(&mut r.nested(body)?)?;\n            {target}.push(mv);\n"
            )
        }
        TyRef::Scalar(FieldType::String) => {
            format!("            {target}.push(r.read_string_owned()?);\n")
        }
        TyRef::Scalar(FieldType::Bytes) => {
            format!("            {target}.push(r.read_length_delimited()?.to_vec());\n")
        }
        TyRef::Scalar(t) if is_varint(*t) => {
            let packed = ctx.syntax != "proto2";
            if packed {
                let from = pack_from(*t, "raw");
                format!(
                    "            match tag.wire_type {{\n                WireType::LengthDelimited => {{\n                    for raw in packed::read_packed_varint(r, |s| {})? {{\n                        {target}.push({from});\n                    }}\n                }}\n                _ => {{\n                    {target}.push({});\n                }}\n            }}\n",
                    dec_scalar_expr_nq(*t, "s"),
                    dec_scalar_expr(*t, "r")
                )
            } else {
                format!("            {target}.push({});\n", dec_scalar_expr(*t, "r"))
            }
        }
        TyRef::Enum(e) => {
            let en = to_rust_type_name(e);
            let packed = ctx.syntax != "proto2";
            if packed {
                format!(
                    "            match tag.wire_type {{\n                WireType::LengthDelimited => {{\n                    for raw in packed::read_packed_varint(r, |s| scalar::read_int32(s))? {{\n                        {target}.push({en}::from_i32(raw));\n                    }}\n                }}\n                _ => {{\n                    {target}.push({en}::from_i32(scalar::read_int32(r)?));\n                }}\n            }}\n"
                )
            } else {
                format!("            {target}.push({en}::from_i32(scalar::read_int32(r)?));\n")
            }
        }
        TyRef::Scalar(t) if is_fixed32(*t) => {
            let packed = ctx.syntax != "proto2";
            if packed {
                let from = pack_from(*t, "raw");
                format!(
                    "            match tag.wire_type {{\n                WireType::LengthDelimited => {{\n                    for raw in packed::read_packed_fixed32(r)? {{\n                        {target}.push({from});\n                    }}\n                }}\n                _ => {{\n                    {target}.push({});\n                }}\n            }}\n",
                    dec_scalar_expr(*t, "r")
                )
            } else {
                format!("            {target}.push({});\n", dec_scalar_expr(*t, "r"))
            }
        }
        TyRef::Scalar(t) if is_fixed64(*t) => {
            let packed = ctx.syntax != "proto2";
            if packed {
                let from = pack_from(*t, "raw");
                format!(
                    "            match tag.wire_type {{\n                WireType::LengthDelimited => {{\n                    for raw in packed::read_packed_fixed64(r)? {{\n                        {target}.push({from});\n                    }}\n                }}\n                _ => {{\n                    {target}.push({});\n                }}\n            }}\n",
                    dec_scalar_expr(*t, "r")
                )
            } else {
                format!("            {target}.push({});\n", dec_scalar_expr(*t, "r"))
            }
        }
        _ => String::new(),
    }
}

fn gen_oneof_encode(
    ctx: &MessageContext<'_>,
    m: &DescriptorProto,
    oi: usize,
    o: &OneofDescriptorProto,
) -> String {
    let oneof_field = to_snake(o.name.as_deref().unwrap_or("oneof"));
    let oneof_enum = oneof_enum_name(ctx, o);
    let mut s = String::new();
    s.push_str(&format!(
        "        if let Some(v) = &self.{oneof_field} {{\n            match v {{\n"
    ));
    for of in m.field.iter().filter(|x| x.oneof_index == Some(oi as i32)) {
        let vname = to_pascal(of.name.as_deref().unwrap_or("field"));
        let num = of.number.unwrap_or(0) as u32;
        let tyref = match TyRef::from_field(of, &ctx.schema.type_map) {
            Some(t) => t,
            None => continue,
        };
        s.push_str(&format!("                {oneof_enum}::{vname}(x) => {{\n"));
        s.push_str(&format!(
            "                    {}\n",
            enc_value("w", num, &tyref, "x".to_string())
        ));
        s.push_str("                }\n");
    }
    s.push_str("            }\n        }\n");
    s
}

/// Build a decode expression for a (non-repeated) value of `tyref` read from
/// reader `r` (a `&mut Reader`). The result is a Rust expression of the field's
/// value type.
fn dec_value_expr(ctx: &MessageContext<'_>, tyref: &TyRef, r: &str) -> String {
    match tyref {
        TyRef::Message(m) => {
            let t = ctx
                .schema
                .type_map
                .get(m)
                .cloned()
                .unwrap_or_else(|| to_rust_type_name(m));
            format!("{{ let body = {r}.read_length_delimited()?; let mut __m = {t}::default(); __m.merge_from(&mut {r}.nested(body)?)?; __m }}")
        }
        TyRef::Enum(e) => {
            let en = ctx
                .schema
                .type_map
                .get(e)
                .cloned()
                .unwrap_or_else(|| to_rust_type_name(e));
            format!("{en}::from_i32(scalar::read_int32({r})?)")
        }
        TyRef::Scalar(t) => dec_scalar_expr(*t, r),
    }
}

fn gen_oneof_decode(
    ctx: &MessageContext<'_>,
    m: &DescriptorProto,
    oi: usize,
    o: &OneofDescriptorProto,
) -> String {
    let oneof_field = to_snake(o.name.as_deref().unwrap_or("oneof"));
    let oneof_enum = oneof_enum_name(ctx, o);
    let mut s = String::new();
    for of in m.field.iter().filter(|x| x.oneof_index == Some(oi as i32)) {
        let num = of.number.unwrap_or(0) as u32;
        let vname = to_pascal(of.name.as_deref().unwrap_or("field"));
        let tyref = match TyRef::from_field(of, &ctx.schema.type_map) {
            Some(t) => t,
            None => continue,
        };
        s.push_str(&format!(
            "                {num} => {{\n                    let x = {};\n                    self.{oneof_field} = Some({oneof_enum}::{vname}(x));\n                }}\n",
            dec_value_expr(ctx, &tyref, "r")
        ));
    }
    s
}

fn gen_map_encode(
    ctx: &MessageContext<'_>,
    f: &FieldDescriptorProto,
    num: u32,
    snake: &str,
) -> String {
    let (k, v) = map_kv(f, ctx.schema);
    let mut s = String::new();
    s.push_str(&format!("        for (k, v) in &self.{snake} {{\n"));
    s.push_str("            let mut __k_tmp = __core::Writer::new();\n");
    s.push_str("            let __k = &mut __k_tmp;\n");
    s.push_str(&format!(
        "            {}\n",
        enc_value("__k", 1, &k, map_key_val_expr(&k, "k"))
    ));
    s.push_str("            let mut __v_tmp = __core::Writer::new();\n");
    s.push_str("            let __v = &mut __v_tmp;\n");
    s.push_str(&format!(
        "            {}\n",
        enc_value("__v", 2, &v, map_val_val_expr(&v, "v"))
    ));
    s.push_str(&format!(
        "            packed::encode_map_entry(w, {num}, __k.buf(), __v.buf());\n"
    ));
    s.push_str("        }\n");
    s
}

fn map_key_val_expr(k: &TyRef, var: &str) -> String {
    let _ = k;
    format!("{var}")
}

fn map_val_val_expr(v: &TyRef, var: &str) -> String {
    let _ = v;
    format!("{var}")
}

fn gen_map_decode(ctx: &MessageContext<'_>, f: &FieldDescriptorProto, snake: &str) -> String {
    let (k, v) = map_kv(f, ctx.schema);
    let mut s = String::new();
    s.push_str("            let body = r.read_length_delimited()?;\n");
    s.push_str("            let (k_raw, v_raw) = packed::decode_map_entry(body)?;\n");
    // key
    s.push_str("            let mut __kr_tmp = Reader::new(&k_raw);\n");
    s.push_str("            let __kr = &mut __kr_tmp;\n");
    let key_expr = match &k {
        TyRef::Scalar(t) => dec_scalar_expr(*t, "__kr"),
        TyRef::Enum(e) => {
            let en = ctx
                .schema
                .type_map
                .get(e)
                .cloned()
                .unwrap_or_else(|| to_rust_type_name(e));
            format!("{en}::from_i32(scalar::read_int32(__kr)?)")
        }
        TyRef::Message(m) => {
            let t = ctx
                .schema
                .type_map
                .get(m)
                .cloned()
                .unwrap_or_else(|| to_rust_type_name(m));
            format!("{{ let body = __kr.read_length_delimited()?; let mut __m = {t}::default(); __m.merge_from(&mut __kr.nested(body)?)?; __m }}")
        }
    };
    s.push_str(&format!("            let k = {key_expr};\n"));
    // value
    s.push_str("            let mut __vr_tmp = Reader::new(&v_raw);\n");
    s.push_str("            let __vr = &mut __vr_tmp;\n");
    let val_expr = match &v {
        TyRef::Message(mv) => {
            let t = ctx
                .schema
                .type_map
                .get(mv)
                .cloned()
                .unwrap_or_else(|| to_rust_type_name(mv));
            format!("{{ let __body = __vr.read_length_delimited()?; let mut __mv = {t}::default(); __mv.merge_from(&mut Reader::new(__body))?; __mv }}")
        }
        TyRef::Enum(e) => {
            let en = ctx
                .schema
                .type_map
                .get(e)
                .cloned()
                .unwrap_or_else(|| to_rust_type_name(e));
            format!("{en}::from_i32(scalar::read_int32(__vr)?)")
        }
        TyRef::Scalar(t) => dec_scalar_expr(*t, "__vr"),
    };
    s.push_str(&format!("            let v = {val_expr};\n"));
    s.push_str(&format!("            self.{snake}.insert(k, v);\n"));
    s
}

/// Generate convenience encode/decode methods on the message struct.
///
/// `decode` reads directly from the borrowed byte slice via a [`Reader`], so it
/// is a zero-copy decode with respect to the underlying buffer (scalar/string/
/// bytes payloads are referenced, never copied, during parsing).
fn gen_convenience_methods(ctx: &MessageContext<'_>) -> String {
    let mut s = String::new();
    s.push_str(&format!("impl {} {{\n", ctx.rust));
    s.push_str(
        "    /// Decode this message from a borrowed byte slice.\n    ///\n    /// The slice is read directly (zero-copy) without copying the buffer\n    /// for scalar, string, or bytes payloads.\n",
    );
    s.push_str("    pub fn decode(buf: &[u8]) -> ::tpt_proto_core::Result<Self> {\n");
    s.push_str("        let mut __msg = Self::default();\n");
    s.push_str("        let mut __r = ::tpt_proto_core::Reader::new(buf);\n");
    s.push_str("        ::tpt_proto_core::Message::merge_from(&mut __msg, &mut __r)?;\n");
    s.push_str("        Ok(__msg)\n");
    s.push_str("    }\n");
    s.push_str(
        "    /// Encode this message into a freshly allocated byte vector.\n",
    );
    s.push_str("    pub fn encode_to_vec(&self) -> ::tpt_proto_core::Result<::std::vec::Vec<u8>> {\n");
    s.push_str("        ::tpt_proto_core::Message::encode_to_vec(self)\n");
    s.push_str("    }\n");
    s.push_str("}\n\n");
    s
}

/// Generate an explicit `impl Default` honoring proto2 field defaults.
fn gen_default_impl(ctx: &MessageContext<'_>, m: &DescriptorProto) -> String {
    let mut s = String::new();
    s.push_str(&format!("impl Default for {} {{\n", ctx.rust));
    s.push_str("    fn default() -> Self {\n");
    s.push_str(&format!("        {} {{\n", ctx.rust));
    for f in &m.field {
        if f.extendee.is_some() {
            continue;
        }
        let snake = to_snake(f.name.as_deref().unwrap_or("field"));
        if f.oneof_index.is_some() {
            s.push_str(&format!("            {snake}: None,\n"));
            continue;
        }
        if is_map_field(f, ctx.schema) {
            s.push_str(&format!(
                "            {snake}: std::collections::HashMap::new(),\n"
            ));
            continue;
        }
        let tyref = match TyRef::from_field(f, &ctx.schema.type_map) {
            Some(t) => t,
            None => {
                s.push_str(&format!("            {snake}: Default::default(),\n"));
                continue;
            }
        };
        let lit = default_literal(f, &tyref, &ctx.schema.type_map);
        s.push_str(&format!("            {snake}: {lit},\n"));
    }
    for o in &m.oneof_decl {
        let snake = to_snake(o.name.as_deref().unwrap_or("oneof"));
        s.push_str(&format!("            {snake}: None,\n"));
    }
    s.push_str("            unknown_fields: Default::default(),\n");
    s.push_str("        }\n    }\n}\n\n");
    s
}

/// A Rust expression for the proto2 `default` value of a field, or
/// `Default::default()` when none is specified.
fn default_literal(
    f: &FieldDescriptorProto,
    tyref: &TyRef,
    type_map: &HashMap<String, String>,
) -> String {
    let dv = match &f.default_value {
        Some(d) => d,
        None => return "Default::default()".to_string(),
    };
    match tyref {
        TyRef::Enum(e) => {
            let en = type_map
                .get(e)
                .cloned()
                .unwrap_or_else(|| to_rust_type_name(e));
            format!("{en}::{}", to_pascal(dv))
        }
        TyRef::Message(_) => "Default::default()".to_string(),
        TyRef::Scalar(t) => match t {
            FieldType::String => format!("{:?}", dv.trim_matches('"')),
            FieldType::Bytes => {
                let inner = unescape_bytes(dv.trim_matches('"'));
                let elems: Vec<String> = inner.iter().map(|b| b.to_string()).collect();
                format!("vec![{}]", elems.join(", "))
            }
            FieldType::Bool => dv.clone(),
            FieldType::Float | FieldType::Double => {
                format!("\"{}\".parse::<f64>().unwrap() as {}", dv, scalar_rust(*t))
            }
            _ => format!("\"{}\".parse::<{}>().unwrap()", dv, scalar_rust(*t)),
        },
    }
}

/// Interpret a protobuf byte-literal string (with C-style escapes) into bytes.
fn unescape_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push(b'\n'),
                Some('t') => out.push(b'\t'),
                Some('r') => out.push(b'\r'),
                Some('0') => out.push(0),
                Some('\\') => out.push(b'\\'),
                Some('\'') => out.push(b'\''),
                Some('"') => out.push(b'"'),
                Some('x') => {
                    let h: String = chars.by_ref().take(2).collect();
                    if let Ok(v) = u8::from_str_radix(&h, 16) {
                        out.push(v);
                    }
                }
                Some(other) => out.push(other as u8),
                None => {}
            }
        } else {
            out.extend_from_slice(c.to_string().as_bytes());
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Builder generation.
// ---------------------------------------------------------------------------

fn gen_builder(ctx: &MessageContext<'_>, m: &DescriptorProto) -> String {
    let name = &ctx.rust;
    let bname = format!("{name}Builder");
    let mut s = String::new();
    s.push_str(&format!("/// Builder for [`{name}`].\n"));
    s.push_str(&format!(
        "pub struct {bname} {{\n    inner: {name},\n}}\n\n"
    ));
    s.push_str(&format!("impl {bname} {{\n"));
    s.push_str(&format!(
        "    /// Create a new builder seeded with default values.\n    pub fn new() -> Self {{\n        Self {{ inner: {name}::default() }}\n    }}\n"
    ));

    for f in &m.field {
        if f.extendee.is_some() {
            continue;
        }
        let snake = to_snake(f.name.as_deref().unwrap_or("field"));
        if is_map_field(f, ctx.schema) {
            let (k, v) = map_kv(f, ctx.schema);
            let kt = k.rust(&ctx.schema.type_map);
            let vt = v.rust(&ctx.schema.type_map);
            s.push_str(&format!(
                "    /// Set the `{snake}` map field.\n    pub fn {snake}(mut self, v: HashMap<{kt}, {vt}>) -> Self {{\n        self.inner.{snake} = v;\n        self\n    }}\n"
            ));
            continue;
        }
        let tyref = match TyRef::from_field(f, &ctx.schema.type_map) {
            Some(t) => t,
            None => continue,
        };
        let base = tyref.rust(&ctx.schema.type_map);
        if f.oneof_index.is_some() {
            let oi = f.oneof_index.unwrap_or(0) as usize;
            let o = &m.oneof_decl[oi];
            let oneof_field = to_snake(o.name.as_deref().unwrap_or("oneof"));
            let oneof_enum = oneof_enum_name(ctx, o);
            let vname = to_pascal(f.name.as_deref().unwrap_or("field"));
            s.push_str(&format!(
                "    /// Set the oneof `{snake}` case.\n    pub fn {snake}(mut self, v: {base}) -> Self {{\n        self.inner.{oneof_field} = Some({oneof_enum}::{vname}(v));\n        self\n    }}\n"
            ));
            continue;
        }
        let arg_ty = match presence(ctx, f) {
            Presence::Repeated => format!("Vec<{base}>"),
            Presence::Explicit => base.clone(),
            Presence::Implicit => base.clone(),
        };
        if matches!(presence(ctx, f), Presence::Explicit) {
            s.push_str(&format!(
                "    /// Set the (optional) `{snake}` field.\n    pub fn {snake}(mut self, v: {base}) -> Self {{\n        self.inner.{snake} = Some(v);\n        self\n    }}\n"
            ));
        } else {
            s.push_str(&format!(
                "    /// Set the `{snake}` field.\n    pub fn {snake}(mut self, v: {arg_ty}) -> Self {{\n        self.inner.{snake} = v;\n        self\n    }}\n"
            ));
        }
    }

    s.push_str(&format!(
        "    /// Build the message, validating required fields.\n    pub fn try_build(self) -> Result<{name}, String> {{\n"
    ));
    for f in &m.field {
        if f.label == Some(Label::Required) && f.extendee.is_none() {
            let snake = to_snake(f.name.as_deref().unwrap_or("field"));
            s.push_str(&format!(
                "        if self.inner.{snake}.is_none() {{ return Err(format!(\"required field '{snake}' is not set\")); }}\n"
            ));
        }
    }
    s.push_str(&format!("        Ok(self.inner)\n    }}\n"));
    s.push_str(&format!(
        "    /// Build the message without required-field validation.\n    pub fn build(self) -> {name} {{\n        self.inner\n    }}\n"
    ));
    s.push_str("}\n\n");
    s
}

// ---------------------------------------------------------------------------
// Service generation (synchronous placeholder traits).
// ---------------------------------------------------------------------------

fn gen_service(
    s: &tpt_proto_descriptor::ServiceDescriptorProto,
    pkg: &str,
    grpc: bool,
) -> String {
    let name = to_pascal(s.name.as_deref().unwrap_or("Service"));
    let svc_name = s.name.clone().unwrap_or_default();
    let full_name = if pkg.is_empty() {
        svc_name.clone()
    } else {
        format!("{pkg}.{svc_name}")
    };

    if !grpc {
        return gen_service_placeholder(&name, &svc_name, s, pkg);
    }
    gen_service_grpc(&name, &full_name, s, pkg)
}

/// Synchronous placeholder trait (used when gRPC generation is disabled).
fn gen_service_placeholder(
    name: &str,
    svc_name: &str,
    s: &tpt_proto_descriptor::ServiceDescriptorProto,
    pkg: &str,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "/// Service trait for `{svc_name}` (synchronous placeholder; enable gRPC codegen for async stubs).\n"
    ));
    out.push_str("#[allow(unused_variables)]\n");
    out.push_str(&format!("pub trait {name} {{\n"));
    for m in &s.method {
        let mname = to_snake(m.name.as_deref().unwrap_or("method"));
        let req = rust_type(m.input_type.as_deref(), pkg);
        let resp = rust_type(m.output_type.as_deref(), pkg);
        out.push_str(&format!(
            "    fn {mname}(&self, req: {req}) -> __core::Result<{resp}>;\n"
        ));
    }
    out.push_str("}\n\n");
    out
}

/// Async gRPC server trait + client stub.
fn gen_service_grpc(
    name: &str,
    full_name: &str,
    s: &tpt_proto_descriptor::ServiceDescriptorProto,
    pkg: &str,
) -> String {
    let mut out = String::new();

    // ---- Server trait ----
    out.push_str(&format!(
        "/// gRPC server trait for `{full_name}`.\n\
         #[async_trait]\n\
         #[allow(unused_variables)]\n\
         pub trait {name}: Send + Sync {{\n"
    ));
    for m in &s.method {
        let mname = to_snake(m.name.as_deref().unwrap_or("method"));
        let req = rust_type(m.input_type.as_deref(), pkg);
        let resp = rust_type(m.output_type.as_deref(), pkg);
        let sig = grpc_method_signature(m, &req, &resp);
        out.push_str(&format!(
            "    /// Handler for `{full_name}.{mname}`.\n    {sig};\n"
        ));
    }
    out.push_str("}\n\n");

    // ---- Client stub ----
    out.push_str(&format!(
        "/// gRPC client stub for `{full_name}`.\n\
         pub struct {name}Client {{\n    \
             pub channel: __grpc::Channel,\n\
         }}\n\n"
    ));
    out.push_str(&format!("impl {name}Client {{\n"));
    out.push_str(
        "    /// Create a new client stub over the given channel.\n    \
         pub fn new(channel: __grpc::Channel) -> Self {\n        \
             Self { channel }\n    }\n",
    );
    for m in &s.method {
        let mname = to_snake(m.name.as_deref().unwrap_or("method"));
        let req = rust_type(m.input_type.as_deref(), pkg);
        let resp = rust_type(m.output_type.as_deref(), pkg);
        let path = format!("/{full_name}/{}", m.name.clone().unwrap_or_default());
        let body = grpc_client_body(m, &req, &resp, &path);
        let sig = grpc_method_signature(m, &req, &resp);
        // Client methods take `&self` and a `Request`, returning the same shape
        // as the server trait (minus `&self`/trait receiver).
        let client_sig = sig.replace("async fn ", "pub async fn ");
        out.push_str(&format!(
            "    /// Call `{full_name}.{mname}`.\n    {client_sig} {{\n{body}    }}\n"
        ));
    }
    out.push_str("}\n\n");

    // ---- Server adapter ----
    out.push_str(&gen_server_adapter(&name, full_name, s, pkg));
    out
}

/// Map a method descriptor to its gRPC streaming `MethodKind` variant name.
fn grpc_kind_name(m: &tpt_proto_descriptor::MethodDescriptorProto) -> &'static str {
    let cs = m.client_streaming.unwrap_or(false);
    let ss = m.server_streaming.unwrap_or(false);
    match (cs, ss) {
        (false, false) => "Unary",
        (false, true) => "ServerStreaming",
        (true, false) => "ClientStreaming",
        (true, true) => "BidiStreaming",
    }
}

/// Generate the `XxxServer<T>` adapter implementing the runtime
/// [`ServiceHandler`] trait for a generated server trait.
fn gen_server_adapter(
    name: &str,
    full_name: &str,
    s: &tpt_proto_descriptor::ServiceDescriptorProto,
    pkg: &str,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "/// gRPC server adapter for `{full_name}`.\n\
         ///\n\
         /// Wraps a user implementation of [`{name}`] and implements the runtime\n\
         /// [`__grpc::ServiceHandler`] trait, translating between typed messages\n\
         /// and the raw bytes the HTTP/2 server dispatches.\n\
         pub struct {name}Server<T: {name} + ?Sized> {{\n    \
             inner: std::sync::Arc<T>,\n\
         }}\n\n"
    ));
    out.push_str(&format!(
        "impl<T: {name} + 'static> {name}Server<T> {{\n    \
             /// Wrap a service implementation.\n    \
             pub fn new(inner: T) -> Self {{ Self {{ inner: std::sync::Arc::new(inner) }} }}\n\
         }}\n\n"
    ));
    out.push_str(&format!(
        "#[async_trait]\n\
         impl<T: {name} + 'static> __grpc::ServiceHandler for {name}Server<T> {{\n    \
             fn full_name(&self) -> &str {{ {full_name:?} }}\n    \
             fn methods(&self) -> std::vec::Vec<(std::string::String, __grpc::MethodKind)> {{\n        \
             std::vec![\n"
    ));
    for m in &s.method {
        let pname = m.name.clone().unwrap_or_default();
        let kind = grpc_kind_name(m);
        out.push_str(&format!(
            "            (std::format!(\"/{full_name}/{pname}\"), __grpc::MethodKind::{kind}),\n"
        ));
    }
    out.push_str("        ]\n    }\n");

    out.push_str(&format!(
        "    async fn call_unary(&self, method: &str, ctx: __grpc::RpcContext, req: std::vec::Vec<u8>) -> std::result::Result<std::vec::Vec<u8>, __grpc::Status> {{\n        \
         match method {{\n{}            _ => Err(__grpc::Status::new(__grpc::Code::Unimplemented, std::format!(\"method {{method}} not found\"))),\n        \
         }}\n    }}\n",
        server_unary_arms(s, pkg)
    ));
    out.push_str(&format!(
        "    async fn call_server_streaming(&self, method: &str, ctx: __grpc::RpcContext, req: std::vec::Vec<u8>) -> std::result::Result<__grpc::ServerStream<std::vec::Vec<u8>>, __grpc::Status> {{\n        \
         match method {{\n{}            _ => Err(__grpc::Status::new(__grpc::Code::Unimplemented, std::format!(\"method {{method}} not found\"))),\n        \
         }}\n    }}\n",
        server_stream_arms(s, pkg, "ServerStreaming")
    ));
    out.push_str(&format!(
        "    async fn call_client_streaming(&self, method: &str, ctx: __grpc::RpcContext, req: __grpc::ClientStream<std::vec::Vec<u8>>) -> std::result::Result<std::vec::Vec<u8>, __grpc::Status> {{\n        \
         match method {{\n{}            _ => Err(__grpc::Status::new(__grpc::Code::Unimplemented, std::format!(\"method {{method}} not found\"))),\n        \
         }}\n    }}\n",
        server_stream_arms(s, pkg, "ClientStreaming")
    ));
    out.push_str(&format!(
        "    async fn call_bidi_streaming(&self, method: &str, ctx: __grpc::RpcContext, req: __grpc::ClientStream<std::vec::Vec<u8>>) -> std::result::Result<__grpc::ServerStream<std::vec::Vec<u8>>, __grpc::Status> {{\n        \
         match method {{\n{}            _ => Err(__grpc::Status::new(__grpc::Code::Unimplemented, std::format!(\"method {{method}} not found\"))),\n        \
         }}\n    }}\n",
        server_stream_arms(s, pkg, "BidiStreaming")
    ));
    out.push_str("}\n\n");
    out
}

/// Match arms for unary dispatch, decoding the request and encoding the reply.
fn server_unary_arms(
    s: &tpt_proto_descriptor::ServiceDescriptorProto,
    pkg: &str,
) -> String {
    let mut arms = String::new();
    for m in &s.method {
        if grpc_kind_name(m) != "Unary" {
            continue;
        }
        let pname = m.name.clone().unwrap_or_default();
        let mname = to_snake(m.name.as_deref().unwrap_or("method"));
        let req = rust_type(m.input_type.as_deref(), pkg);
        let _resp = rust_type(m.output_type.as_deref(), pkg);
        arms.push_str(&format!(
            "            {pname:?} => {{\n                \
             let msg = <{req}>::decode(&req).map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string()))?;\n                \
             let resp = self.inner.{mname}(__grpc::Request::with_context(msg, ctx)).await?;\n                \
             resp.message.encode_to_vec().map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string()))\n            }},\n"
        ));
    }
    arms
}

/// Match arms for streaming dispatch (server/client/bidi).
fn server_stream_arms(
    s: &tpt_proto_descriptor::ServiceDescriptorProto,
    pkg: &str,
    kind: &str,
) -> String {
    let mut arms = String::new();
    for m in &s.method {
        if grpc_kind_name(m) != kind {
            continue;
        }
        let pname = m.name.clone().unwrap_or_default();
        let mname = to_snake(m.name.as_deref().unwrap_or("method"));
        let req = rust_type(m.input_type.as_deref(), pkg);
        let _resp = rust_type(m.output_type.as_deref(), pkg);
        let arm = match kind {
            "ServerStreaming" => format!(
                "            {pname:?} => {{\n                \
                 let msg = <{req}>::decode(&req).map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string()))?;\n                \
                 let resp = self.inner.{mname}(__grpc::Request::with_context(msg, ctx)).await?;\n                \
                 let mapped = __grpc::framed::map_server_stream(resp.message, |m| m.encode_to_vec().map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string())));\n                \
                 Ok(mapped)\n            }},\n"
            ),
            "ClientStreaming" => format!(
                "            {pname:?} => {{\n                \
                 let req_stream = __grpc::framed::map_client_stream(req, |b| <{req}>::decode(&b).map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string())));\n                \
                 let resp = self.inner.{mname}(__grpc::Request::with_context(req_stream, ctx)).await?;\n                \
                 resp.message.encode_to_vec().map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string()))\n            }},\n"
            ),
            "BidiStreaming" => format!(
                "            {pname:?} => {{\n                \
                 let req_stream = __grpc::framed::map_client_stream(req, |b| <{req}>::decode(&b).map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string())));\n                \
                 let resp = self.inner.{mname}(__grpc::Request::with_context(req_stream, ctx)).await?;\n                \
                 let mapped = __grpc::framed::map_server_stream(resp.message, |m| m.encode_to_vec().map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string())));\n                \
                 Ok(mapped)\n            }},\n"
            ),
            _ => String::new(),
        };
        arms.push_str(&arm);
    }
    arms
}

/// The async method signature shared by the server trait and client stub.
fn grpc_method_signature(
    m: &tpt_proto_descriptor::MethodDescriptorProto,
    req: &str,
    resp: &str,
) -> String {
    let mname = to_snake(m.name.as_deref().unwrap_or("method"));
    let cs = m.client_streaming.unwrap_or(false);
    let ss = m.server_streaming.unwrap_or(false);
    let req_ty = if cs {
        format!("__grpc::Request<__grpc::ClientStream<{req}>>")
    } else {
        format!("__grpc::Request<{req}>")
    };
    let ret_ty = if ss {
        format!("__grpc::Response<__grpc::ServerStream<{resp}>>")
    } else {
        format!("__grpc::Response<{resp}>")
    };
    format!("async fn {mname}(&self, request: {req_ty}) -> std::result::Result<{ret_ty}, __grpc::Status>")
}

/// The client-stub method body.
fn grpc_client_body(
    m: &tpt_proto_descriptor::MethodDescriptorProto,
    req: &str,
    resp: &str,
    path: &str,
) -> String {
    let cs = m.client_streaming.unwrap_or(false);
    let ss = m.server_streaming.unwrap_or(false);
    if !cs && !ss {
        // Unary: the channel frames the request and deframes the response.
        return format!(
            "        let (message, trailers) = self.channel\n            \
         .unary::<{req}, {resp}>(\n                \
         {path:?},\n                \
         request.context.metadata.clone(),\n                \
         &request.message,\n            \
         )\n            \
         .await?;\n        \
         Ok(__grpc::Response::new(message).with_metadata(trailers))\n"
        );
    }
    if !cs && ss {
        // Server streaming: one request message, a stream of responses.
        return format!(
            "        let req_raw = request.message.encode_to_vec().map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string()))?;\n        \
             let (stream, trailers) = self.channel.transport().server_streaming(\n            \
             {path:?},\n            \
             request.context.metadata.clone(),\n            \
             req_raw,\n        \
             ).await?;\n        \
             let mapped = __grpc::framed::map_server_stream(stream, |bytes| {{\n            \
             <{resp}>::decode(&bytes).map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string()))\n        \
             }});\n        \
             Ok(__grpc::Response::new(mapped).with_metadata(trailers))\n"
        );
    }
    if cs && !ss {
        // Client streaming: a stream of requests, one response.
        return format!(
            "        let req_stream = __grpc::framed::map_client_stream(request.message, |msg| {{\n            \
             msg.encode_to_vec().map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string()))\n        \
             }});\n        \
             let (message, trailers) = self.channel.transport().client_streaming(\n            \
             {path:?},\n            \
             request.context.metadata.clone(),\n            \
             req_stream,\n        \
             ).await?;\n        \
             let resp = <{resp}>::decode(&message).map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string()))?;\n        \
             Ok(__grpc::Response::new(resp).with_metadata(trailers))\n"
        );
    }
    // Bidi streaming.
    format!(
        "        let req_stream = __grpc::framed::map_client_stream(request.message, |msg| {{\n            \
         msg.encode_to_vec().map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string()))\n        \
         }});\n        \
         let (stream, trailers) = self.channel.transport().bidi_streaming(\n            \
         {path:?},\n            \
         request.context.metadata.clone(),\n            \
         req_stream,\n        \
         ).await?;\n        \
         let mapped = __grpc::framed::map_server_stream(stream, |bytes| {{\n            \
         <{resp}>::decode(&bytes).map_err(|e| __grpc::Status::new(__grpc::Code::Internal, e.to_string()))\n        \
         }});\n        \
         Ok(__grpc::Response::new(mapped).with_metadata(trailers))\n"
    )
}

/// Map a fully-qualified proto type name to a Rust type name, defaulting to
/// Map a fully-qualified proto type name to a Rust type name, defaulting to
/// `()` for absent types.
///
/// The lightweight [`tpt_proto_compiler::compile`] used by the code generator
/// leaves service method types as simple names; this resolves them against the
/// enclosing package so they line up with the generated message types (which
/// are named by fully-qualified proto name).
fn rust_type(t: Option<&str>, pkg: &str) -> String {
    let Some(name) = t else {
        return "()".to_string();
    };
    let trimmed = name.trim_start_matches('.');
    if trimmed.contains('.') {
        // Already fully qualified (e.g. resolved by the full pipeline).
        to_rust_type_name(name)
    } else {
        // Simple name in the current package.
        let fqn = if pkg.is_empty() {
            format!(".{name}")
        } else {
            format!(".{}.{name}", pkg.trim_start_matches('.'))
        };
        to_rust_type_name(&fqn)
    }
}

// ---------------------------------------------------------------------------
// Convenience: generate from parsed proto source.
// ---------------------------------------------------------------------------

/// Parse and compile `source` (a single proto file) then generate Rust code.
///
/// This is a convenience wrapper used by tests and tooling; multi-file
/// compilation should use [`tpt_proto_compiler::compile_set`] directly.
pub fn generate_from_source(
    name: &str,
    source: &str,
    options: &GenerateOptions,
) -> Result<String, CodegenError> {
    let parsed = tpt_proto_language::parse_file(name, source);
    if parsed.diagnostics.has_errors() {
        return Err(CodegenError::Unsupported(format!(
            "parse errors: {:?}",
            parsed.diagnostics.iter().collect::<Vec<_>>()
        )));
    }
    let (fd, _diags) = tpt_proto_compiler::compile(&parsed.file);
    let set = FileDescriptorSet { file: vec![fd] };
    generate(&set, options)
}

/// A comprehensive proto3 sample schema used by the generator's own tests and
/// by the example/integration harness.
pub const SAMPLE_PROTO: &str = r#"
syntax = "proto3";
package sample;

message Scalars {
  double d = 1;
  float f = 2;
  int32 i32 = 3;
  int64 i64 = 4;
  uint32 u32 = 5;
  uint64 u64 = 6;
  sint32 s32 = 7;
  sint64 s64 = 8;
  fixed32 f32 = 9;
  fixed64 f64 = 10;
  sfixed32 sf32 = 11;
  sfixed64 sf64 = 12;
  bool b = 13;
  string s = 14;
  bytes by = 15;
  Nested nested = 16;
}

message Nested {
  string name = 1;
}

enum Color {
  RED = 0;
  GREEN = 1;
  BLUE = 2;
}

message Collections {
  repeated int32 nums = 1;
  repeated string strs = 2;
  repeated Nested items = 3;
  map<string, int32> labels = 4;
  map<int32, Nested> by_id = 5;
  Color color = 6;
  repeated Color colors = 7;
}

message WithOneof {
  oneof choice {
    string a = 1;
    int32 n = 2;
    Nested obj = 3;
  }
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
syntax = "proto3";
package example;

message Person {
  string name = 1;
  int32 id = 2;
  repeated string emails = 3;
  optional Address address = 4;

  enum PhoneType {
    MOBILE = 0;
    HOME = 1;
    WORK = 2;
  }

  message Address {
    string city = 1;
  }

  oneof contact {
    string email = 5;
    string phone = 6;
  }

  map<string, int32> labels = 7;
}

service Directory {
  rpc Lookup(Person) returns (Person);
}
"#;

    #[test]
    fn generates_without_error() {
        let code = generate_from_source("example.proto", SRC, &GenerateOptions::default())
            .expect("codegen should succeed");
        assert!(code.contains("pub struct ExamplePerson"));
        assert!(code.contains("pub enum ExamplePersonPhoneType"));
        assert!(code.contains("pub struct ExamplePersonAddress"));
        assert!(code.contains("ExamplePerson_Contact"));
        assert!(code.contains("impl Message for ExamplePerson"));
        assert!(code.contains("ExamplePersonBuilder"));
        assert!(code.contains("pub trait Directory"));
    }
}
