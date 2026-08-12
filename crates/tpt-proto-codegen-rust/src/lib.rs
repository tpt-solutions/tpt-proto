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
    DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FieldType,
    FileDescriptorSet, Label, OneofDescriptorProto,
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
fn dec_scalar_expr(t: FieldType, r: &str) -> String {
    let m = |name: &str| format!("scalar::{name}(&mut {r})?");
    match t {
        FieldType::Int32 => m("read_int32"),
        FieldType::Int64 => m("read_int64"),
        FieldType::Uint32 => m("read_uint32"),
        FieldType::Uint64 => m("read_uint64"),
        FieldType::Bool => m("read_bool"),
        FieldType::Sint32 => m("read_sint32"),
        FieldType::Sint64 => m("read_sint64"),
        FieldType::Fixed32 => m("read_fixed32"),
        FieldType::Sfixed32 => m("read_sfixed32"),
        FieldType::Fixed64 => m("read_fixed64"),
        FieldType::Sfixed64 => m("read_sfixed64"),
        FieldType::Float => m("read_float"),
        FieldType::Double => m("read_double"),
        // String/bytes/message are handled by dedicated paths.
        _ => format!("unreachable!()"),
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
    matches!(t, FieldType::Fixed32 | FieldType::Sfixed32 | FieldType::Float)
}

/// Whether a scalar type is a fixed-width 64-bit value.
fn is_fixed64(t: FieldType) -> bool {
    matches!(t, FieldType::Fixed64 | FieldType::Sfixed64 | FieldType::Double)
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
        FieldType::Message | FieldType::Group => {
            f.type_name.clone().map(TyRef::Message)
        }
        FieldType::Enum => f.type_name.clone().map(TyRef::Enum),
        other => Some(TyRef::Scalar(other)),
    }
}

// ---------------------------------------------------------------------------
// Code generation entry point.
// ---------------------------------------------------------------------------

/// Generate Rust source code for an entire [`FileDescriptorSet`].
pub fn generate(set: &FileDescriptorSet, _options: &GenerateOptions) -> Result<String, CodegenError> {
    let schema = Schema::build(set);
    let mut out = String::new();

    out.push_str(
        "// @generated by tpt-proto-codegen-rust. DO NOT EDIT.\n\
         #![allow(non_camel_case_types, non_snake_case, unused_imports, clippy::all, clippy::derive_partial_eq_without_eq)]\n\
         use tpt_proto_core as __core;\n\
         use __core::{Message, Reader, Writer, UnknownFieldSet, WireType};\n\
         use __core::scalar;\n\
         use __core::packed;\n\
         use std::collections::HashMap;\n\n",
    );

    // Enums first so messages can reference them.
    for file in &set.file {
        let pkg = file.package.clone().unwrap_or_default();
        for e in &file.enum_type {
            out.push_str(&gen_enum(&qualify(&pkg, &e.name.clone().unwrap_or_default()), e));
        }
        for m in &file.message_type {
            gen_enum_nested(&pkg, m, &schema, &mut out);
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

    // Services (synchronous placeholder traits; expanded by gRPC codegen).
    for file in &set.file {
        for s in &file.service {
            out.push_str(&gen_service(s));
        }
    }

    Ok(out)
}

fn gen_enum_nested(prefix: &str, m: &DescriptorProto, schema: &Schema, out: &mut String) {
    let fqn = qualify(prefix, &m.name.clone().unwrap_or_default());
    if schema.skip.contains(&fqn) {
        // Map entries are not emitted; still recurse for any nested types.
        for n in &m.nested_type {
            gen_enum_nested(&fqn, n, schema, out);
        }
        for e in &m.enum_type {
            let efqn = qualify(&fqn, &e.name.clone().unwrap_or_default());
            out.push_str(&gen_enum(&efqn, e));
        }
        return;
    }
    for e in &m.enum_type {
        let efqn = qualify(&fqn, &e.name.clone().unwrap_or_default());
        out.push_str(&gen_enum(&efqn, e));
    }
    for n in &m.nested_type {
        gen_enum_nested(&fqn, n, schema, out);
    }
}

fn gen_enum(fqn: &str, e: &EnumDescriptorProto) -> String {
    let rust = to_rust_type_name(fqn);
    let mut s = String::new();
    s.push_str(&format!("#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]\n"));
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

    // Oneof enums.
    for (oi, o) in m.oneof_decl.iter().enumerate() {
        out.push_str(&gen_oneof_enum(&ctx, m, oi, o));
    }

    // Struct definition.
    let mut struct_def = String::new();
    struct_def.push_str(&format!("#[derive(Debug, Clone, PartialEq, Eq, Default)]\n"));
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
            let rust_ty = field_rust_type(&ctx, f, &tyref);
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

    // Message impl with encode/decode.
    out.push_str(&gen_message_impl(&ctx, m));

    // Builder.
    out.push_str(&gen_builder(&ctx, m));

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

/// A field's generated Rust type, honoring label/presence/oneof/map.
fn field_rust_type(ctx: &MessageContext<'_>, f: &FieldDescriptorProto, tyref: &TyRef) -> String {
    let base = tyref.rust(&ctx.schema.type_map);
    match f.label {
        Some(Label::Repeated) => format!("Vec<{base}>"),
        Some(Label::Required) | Some(Label::Optional) => {
            if f.proto3_optional == Some(true) {
                format!("Option<{base}>")
            } else if ctx.syntax == "proto3" {
                // proto3 singular: implicit presence (no Option).
                base
            } else {
                // proto2 / editions: explicit presence.
                format!("Option<{base}>")
            }
        }
        None => base,
    }
}

fn is_map_field(f: &FieldDescriptorProto, schema: &Schema) -> bool {
    f.label == Some(Label::Repeated)
        && f.r#type == Some(FieldType::Message)
        && f.type_name.as_ref().map_or(false, |t| schema.map_entries.contains_key(t))
}

fn map_kv(f: &FieldDescriptorProto, schema: &Schema) -> (TyRef, TyRef) {
    let entry = f
        .type_name
        .as_ref()
        .and_then(|t| schema.map_entries.get(t));
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
    s.push_str("#[derive(Debug, Clone, PartialEq, Eq)]\n");
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
        if f.extendee.is_some() {
            continue;
        }
        let num = f.number.unwrap_or(0) as u32;
        let snake = to_snake(f.name.as_deref().unwrap_or("field"));
        if is_map_field(f, ctx.schema) {
            s.push_str(&gen_map_encode(ctx, f, num, &snake));
            continue;
        }
        if f.oneof_index.is_some() {
            s.push_str(&gen_oneof_encode(ctx, m, f, num, &snake));
            continue;
        }
        let tyref = match TyRef::from_field(f, &ctx.schema.type_map) {
            Some(t) => t,
            None => continue,
        };
        s.push_str(&gen_field_encode(ctx, f, &tyref, num, &snake));
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
        if f.extendee.is_some() {
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
        if f.oneof_index.is_some() {
            s.push_str(&format!("{num} => {{\n"));
            s.push_str(&gen_oneof_decode(ctx, m, f, &snake));
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
                    enc_value("w", num, tyref, "self.".to_string() + snake)
                ));
                s.push_str("        }\n");
            } else {
                s.push_str(&format!(
                    "        {}\n",
                    enc_value("w", num, tyref, "self.".to_string() + snake)
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
                "        if let Some(v) = &self.{snake} {{\n            scalar::encode_message(w, {num}, &v.encode_to_vec());\n        }}\n",
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
        TyRef::Scalar(t) => match t {
            FieldType::String => format!("scalar::encode_string({buf}, {num}, {val_expr});"),
            FieldType::Bytes => format!("scalar::encode_bytes({buf}, {num}, {val_expr});"),
            FieldType::Enum => {
                format!("scalar::encode_enum({buf}, {num}, {val_expr}.as_i32());")
            }
            FieldType::Message | FieldType::Group => {
                format!("scalar::encode_message({buf}, {num}, &{val_expr}.encode_to_vec());")
            }
            other => format!("scalar::{}({buf}, {num}, *({val_expr}));", enc_scalar_fn(*other)),
        },
        TyRef::Enum(_) => format!("scalar::encode_enum({buf}, {num}, {val_expr}.as_i32());"),
        TyRef::Message(_) => {
            format!("scalar::encode_message({buf}, {num}, &{val_expr}.encode_to_vec());")
        }
    }
}

fn gen_repeated_encode(tyref: &TyRef, num: u32, snake: &str, ctx: &MessageContext<'_>) -> String {
    let target = format!("self.{snake}");
    match tyref {
        TyRef::Message(_) => format!(
            "        for v in &{target} {{\n            scalar::encode_message(w, {num}, &v.encode_to_vec());\n        }}\n"
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
    !is_fixed32(t) && !is_fixed64(t)
        && !matches!(t, FieldType::String | FieldType::Bytes | FieldType::Message | FieldType::Group)
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
                    "            let body = {r}.read_length_delimited()?;\n            {target}.get_or_insert_with({}::default).merge_from(&mut Reader::new(body))?;\n",
                    to_rust_type_name(&t)
                )
            } else {
                format!(
                    "            {target} = {}::default();\n            {target}.merge_from(&mut Reader::new({r}.read_length_delimited()?))?;\n",
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
                format!(
                    "            {target} = Some({en}::from_i32(scalar::read_int32(&mut {r})?));\n"
                )
            } else {
                format!(
                    "            {target} = {en}::from_i32(scalar::read_int32(&mut {r})?);\n"
                )
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
                "            let body = r.read_length_delimited()?;\n            let mut mv = {t}::default();\n            mv.merge_from(&mut Reader::new(body))?;\n            {target}.push(mv);\n"
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
                    dec_scalar_expr(*t, "s"),
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
                format!(
                    "            {target}.push({en}::from_i32(scalar::read_int32(r)?));\n"
                )
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
    f: &FieldDescriptorProto,
    num: u32,
    _snake: &str,
) -> String {
    let oi = f.oneof_index.unwrap_or(0) as usize;
    let o = &m.oneof_decl[oi];
    let oneof_field = to_snake(o.name.as_deref().unwrap_or("oneof"));
    let oneof_enum = oneof_enum_name(ctx, o);
    let mut s = String::new();
    s.push_str(&format!(
        "        if let Some(v) = &self.{oneof_field} {{\n            match v {{\n"
    ));
    for of in m.field.iter().filter(|x| x.oneof_index == Some(oi as i32)) {
        let vname = to_pascal(of.name.as_deref().unwrap_or("field"));
        s.push_str(&format!("                {oneof_enum}::{vname}(x) => {{\n"));
        let tyref = match TyRef::from_field(of, &ctx.schema.type_map) {
            Some(t) => t,
            None => continue,
        };
        s.push_str(&format!("                    {}\n", enc_value("w", num, &tyref, "x".to_string())));
        s.push_str("                }\n");
    }
    s.push_str("            }\n        }\n");
    s
}

fn gen_oneof_decode(
    ctx: &MessageContext<'_>,
    m: &DescriptorProto,
    f: &FieldDescriptorProto,
    snake: &str,
) -> String {
    let oi = f.oneof_index.unwrap_or(0) as usize;
    let o = &m.oneof_decl[oi];
    let oneof_field = to_snake(o.name.as_deref().unwrap_or("oneof"));
    let oneof_enum = oneof_enum_name(ctx, o);
    let mut s = String::new();
    let vname = to_pascal(f.name.as_deref().unwrap_or("field"));
    let tyref = match TyRef::from_field(f, &ctx.schema.type_map) {
        Some(t) => t,
        None => return s,
    };
    s.push_str(&format!("            let x = {{\n"));
    // Decode value into a temporary based on type.
    match &tyref {
        TyRef::Message(mf) => {
            let t = to_rust_type_name(mf);
            s.push_str(&format!(
                "                let body = r.read_length_delimited()?;\n                let mut tmp = {t}::default();\n                tmp.merge_from(&mut Reader::new(body))?;\n                tmp\n"
            ));
        }
        TyRef::Scalar(FieldType::String) => {
            s.push_str("                r.read_string_owned()?\n");
        }
        TyRef::Scalar(FieldType::Bytes) => {
            s.push_str("                r.read_length_delimited()?.to_vec()\n");
        }
        TyRef::Scalar(t) => {
            s.push_str(&format!("                {}\n", dec_scalar_expr(*t, "r")));
        }
        TyRef::Enum(e) => {
            let en = to_rust_type_name(e);
            s.push_str(&format!(
                "                {en}::from_i32(scalar::read_int32(&mut r)?)\n"
            ));
        }
    }
    s.push_str(&format!(
        "            }};\n            self.{snake} = Some({oneof_enum}::{vname}(x));\n"
    ));
    s
}

fn gen_map_encode(ctx: &MessageContext<'_>, f: &FieldDescriptorProto, num: u32, snake: &str) -> String {
    let (k, v) = map_kv(f, ctx.schema);
    let mut s = String::new();
    s.push_str(&format!("        for (k, v) in &self.{snake} {{\n"));
    s.push_str("            let mut __k = __core::Writer::new();\n");
    s.push_str(&format!(
        "            {}\n",
        enc_value("__k", 1, &k, map_key_val_expr(&k, "k"))
    ));
    s.push_str("            let mut __v = __core::Writer::new();\n");
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
    s.push_str("            let mut __kr = Reader::new(&k_raw);\n");
    let key_expr = match &k {
        TyRef::Scalar(t) => dec_scalar_expr(*t, "__kr"),
        _ => dec_scalar_expr(FieldType::String, "__kr"),
    };
    s.push_str(&format!("            let k = {key_expr};\n"));
    // value
    s.push_str("            let mut __vr = Reader::new(&v_raw);\n");
    let val_expr = match &v {
        TyRef::Message(mv) => {
            let t = to_rust_type_name(mv);
            format!("{{ let mut __v = {t}::default(); __v.merge_from(&mut Reader::new(&v_raw))?; __v }}")
        }
        TyRef::Enum(e) => {
            let en = to_rust_type_name(e);
            format!("{en}::from_i32(scalar::read_int32(&mut __vr)?)")
        }
        TyRef::Scalar(t) => dec_scalar_expr(*t, "__vr"),
    };
    s.push_str(&format!("            let v = {val_expr};\n"));
    s.push_str(&format!("            self.{snake}.insert(k, v);\n"));
    s
}

// ---------------------------------------------------------------------------
// Builder generation.
// ---------------------------------------------------------------------------

fn gen_builder(ctx: &MessageContext<'_>, m: &DescriptorProto) -> String {
    let name = &ctx.rust;
    let bname = format!("{name}Builder");
    let mut s = String::new();
    s.push_str(&format!("/// Builder for [`{name}`].\n"));
    s.push_str(&format!("pub struct {bname} {{\n    inner: {name},\n}}\n\n"));
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

fn gen_service(s: &tpt_proto_descriptor::ServiceDescriptorProto) -> String {
    let name = to_pascal(s.name.as_deref().unwrap_or("Service"));
    let mut out = String::new();
    out.push_str(&format!(
        "/// Service trait for `{}` (synchronous placeholder; expanded by gRPC codegen).\n",
        s.name.clone().unwrap_or_default()
    ));
    out.push_str("#[allow(unused_variables)]\n");
    out.push_str(&format!("pub trait {name} {{\n"));
    for m in &s.method {
        let mname = to_snake(m.name.as_deref().unwrap_or("method"));
        let req = m
            .input_type
            .as_ref()
            .map(|t| to_rust_type_name(t))
            .unwrap_or_else(|| "()".to_string());
        let resp = m
            .output_type
            .as_ref()
            .map(|t| to_rust_type_name(t))
            .unwrap_or_else(|| "()".to_string());
        out.push_str(&format!(
            "    fn {mname}(&self, req: {req}) -> __core::Result<{resp}>;\n"
        ));
    }
    out.push_str("}\n\n");
    out
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
        assert!(code.contains("pub enum ExamplePhoneType"));
        assert!(code.contains("pub struct ExamplePersonAddress"));
        assert!(code.contains("ExamplePerson_contact"));
        assert!(code.contains("impl Message for ExamplePerson"));
        assert!(code.contains("ExamplePersonBuilder"));
        assert!(code.contains("pub trait Directory"));
    }
}
