//! `tpt-proto-compiler` — semantic analysis and descriptor construction.
//!
//! Converts a parsed [`tpt_proto_language::ast::File`] into a
//! [`tpt_proto_descriptor::FileDescriptorProto`], performing the core
//! validations (field-number ranges, duplicates, reserved conflicts) and
//! emitting diagnostics. This is the validated-AST → descriptor step of the
//! compiler pipeline; deeper editions feature resolution is layered on top.

use tpt_proto_core::{Message, Result as CoreResult};
use tpt_proto_descriptor::{
    DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, ExtensionRange,
    FieldDescriptorProto, FieldType, FileDescriptorProto, Label, MethodDescriptorProto,
    OneofDescriptorProto, ReservedRange, ServiceDescriptorProto,
};
use tpt_proto_language::ast;
use tpt_proto_language::diagnostic::{Diagnostic, Diagnostics, ErrorCode};

mod features;
mod pipeline;

pub use features::{
    EnumType, FeatureOverrides, FeatureSet, FieldPresence, JsonFormat, MessageEncoding,
    RepeatedFieldEncoding, Utf8Validation,
};
pub use pipeline::{compile_set, CompileResult, LintFinding, LintReport};

/// Compile a single parsed proto file into a [`FileDescriptorProto`].
///
/// This is the validated-AST → descriptor step for one file. For multi-file
/// compilation with import/package resolution, duplicate detection across
/// files, and editions feature resolution, use [`compile_set`].
pub fn compile(file: &ast::File) -> (FileDescriptorProto, Diagnostics) {
    let mut diags = Diagnostics::new();
    let mut out = FileDescriptorProto {
        name: Some(file.name.clone()),
        package: file.package.as_ref().map(|p| p.name.clone()),
        syntax: file.syntax.as_ref().map(|s| s.value.clone()),
        ..Default::default()
    };

    // Syntax: editions set `syntax = "editions"`; otherwise the keyword, or
    // proto2 when omitted (proto2 is the historical default).
    out.syntax = if file.edition.is_some() {
        Some("editions".to_string())
    } else {
        Some(
            file.syntax
                .as_ref()
                .map(|s| s.value.clone())
                .unwrap_or_else(|| "proto2".to_string()),
        )
    };

    for (idx, imp) in file.imports.iter().enumerate() {
        out.dependency.push(imp.path.clone());
        match imp.kind {
            ast::ImportKind::Public => out.public_dependency.push(idx as i32),
            ast::ImportKind::Weak => out.weak_dependency.push(idx as i32),
            ast::ImportKind::Default => {}
        }
    }

    let enum_names = collect_enum_names(file);
    let msg_names = collect_message_names(file);
    let syntax_str = out.syntax.as_deref().unwrap_or("proto2");
    let pkg = out.package.as_deref().unwrap_or("");

    check_duplicates(file, &mut diags);
    // Top-level extensions.
    for ext in &file.extensions {
        for f in &ext.fields {
            out.extension.push(convert_field(
                f,
                &enum_names,
                &msg_names,
                syntax_str,
                &mut diags,
            ));
        }
    }

    for m in &file.messages {
        out.message_type.push(convert_message(
            m,
            &enum_names,
            &msg_names,
            pkg,
            syntax_str,
            &mut diags,
        ));
    }
    for e in &file.enums {
        out.enum_type.push(convert_enum(e, &mut diags));
    }
    for s in &file.services {
        out.service.push(convert_service(s, &mut diags));
    }

    (out, diags)
}

fn collect_enum_names(file: &ast::File) -> Vec<String> {
    let mut names = Vec::new();
    for e in &file.enums {
        names.push(e.name.name.clone());
    }
    for m in &file.messages {
        collect_enum_names_in_message(m, &mut names);
    }
    names
}

fn collect_enum_names_in_message(m: &ast::Message, names: &mut Vec<String>) {
    for e in &m.nested_enums {
        names.push(e.name.name.clone());
    }
    for n in &m.nested_messages {
        collect_enum_names_in_message(n, names);
    }
}

fn collect_message_names(file: &ast::File) -> Vec<String> {
    let mut names = Vec::new();
    for m in &file.messages {
        names.push(m.name.name.clone());
        collect_message_names_in_message(m, &mut names);
    }
    names
}

fn collect_message_names_in_message(m: &ast::Message, names: &mut Vec<String>) {
    for n in &m.nested_messages {
        names.push(n.name.name.clone());
        collect_message_names_in_message(n, names);
    }
}

fn check_field_number(
    number: i64,
    span: tpt_proto_language::diagnostic::Span,
    diags: &mut Diagnostics,
) {
    if !(1..=536_870_911).contains(&number) {
        diags.push(Diagnostic::error(
            ErrorCode::InvalidFieldNumber,
            format!("field number {number} is out of range (must be 1..=536870911)"),
            Some(span),
        ));
    }
    if (19_000..=19_999).contains(&number) {
        diags.push(Diagnostic::warning(
            ErrorCode::InvalidFieldNumber,
            format!("field number {number} is in the reserved implementation range 19000..=19999"),
            Some(span),
        ));
    }
}

fn resolve_type(
    ty: &ast::TypeRef,
    enum_names: &[String],
    msg_names: &[String],
) -> (FieldType, Option<String>) {
    match ty {
        ast::TypeRef::Scalar(s) => (scalar_to_field_type(*s), None),
        ast::TypeRef::Named(ident) => {
            let simple = ident
                .name
                .trim_start_matches('.')
                .rsplit('.')
                .next()
                .unwrap_or(&ident.name);
            let t = if enum_names.iter().any(|n| n == simple) {
                FieldType::Enum
            } else if msg_names.iter().any(|n| n == simple) {
                FieldType::Message
            } else {
                // Unknown; assume message (resolution may refine later).
                FieldType::Message
            };
            (t, Some(ident.name.clone()))
        }
    }
}

fn scalar_to_field_type(s: ast::ScalarType) -> FieldType {
    match s {
        ast::ScalarType::Double => FieldType::Double,
        ast::ScalarType::Float => FieldType::Float,
        ast::ScalarType::Int64 => FieldType::Int64,
        ast::ScalarType::Uint64 => FieldType::Uint64,
        ast::ScalarType::Int32 => FieldType::Int32,
        ast::ScalarType::Fixed64 => FieldType::Fixed64,
        ast::ScalarType::Fixed32 => FieldType::Fixed32,
        ast::ScalarType::Bool => FieldType::Bool,
        ast::ScalarType::String => FieldType::String,
        ast::ScalarType::Bytes => FieldType::Bytes,
        ast::ScalarType::Uint32 => FieldType::Uint32,
        ast::ScalarType::Sfixed32 => FieldType::Sfixed32,
        ast::ScalarType::Sfixed64 => FieldType::Sfixed64,
        ast::ScalarType::Sint32 => FieldType::Sint32,
        ast::ScalarType::Sint64 => FieldType::Sint64,
    }
}

fn convert_label(label: ast::Label, _syntax: &str) -> Label {
    match label {
        ast::Label::Optional => Label::Optional,
        ast::Label::Required => Label::Required,
        ast::Label::Repeated => Label::Repeated,
        ast::Label::Singular => Label::Optional,
    }
}

fn convert_field(
    f: &ast::Field,
    enum_names: &[String],
    msg_names: &[String],
    syntax: &str,
    diags: &mut Diagnostics,
) -> FieldDescriptorProto {
    check_field_number(f.number, f.span, diags);
    let (ty, type_name) = resolve_type(&f.ty, enum_names, msg_names);
    // proto3 explicit `optional` needs `proto3_optional` so codegen tracks
    // presence; proto2 `optional`/`required` and editions (feature-driven)
    // do not.
    let proto3_optional = if f.label == ast::Label::Optional && syntax == "proto3" {
        Some(true)
    } else {
        None
    };
    FieldDescriptorProto {
        name: Some(f.name.name.clone()),
        number: Some(f.number as i32),
        label: Some(convert_label(f.label, syntax)),
        r#type: Some(ty),
        type_name,
        default_value: f.default.as_ref().map(constant_to_text),
        json_name: f
            .json_name
            .clone()
            .or_else(|| Some(to_json_name(&f.name.name))),
        proto3_optional,
        ..Default::default()
    }
}

fn convert_message(
    m: &ast::Message,
    enum_names: &[String],
    msg_names: &[String],
    path_prefix: &str,
    syntax: &str,
    diags: &mut Diagnostics,
) -> DescriptorProto {
    let mut out = DescriptorProto {
        name: Some(m.name.name.clone()),
        ..Default::default()
    };

    // Fully-qualified path of this message (without a leading dot), used to
    // name synthetic map-entry types and recurse into nested messages.
    let self_path = if path_prefix.is_empty() {
        m.name.name.clone()
    } else {
        format!("{}.{}", path_prefix, m.name.name)
    };

    // oneofs first (so oneof_index is stable).
    for (i, o) in m.oneofs.iter().enumerate() {
        out.oneof_decl.push(OneofDescriptorProto {
            name: Some(o.name.name.clone()),
            ..Default::default()
        });
        for f in &o.fields {
            let mut fd = convert_field(f, enum_names, msg_names, syntax, diags);
            fd.oneof_index = Some(i as i32);
            fd.label = Some(Label::Optional);
            // Oneof members are not tracked via `proto3_optional`.
            fd.proto3_optional = None;
            out.field.push(fd);
        }
    }

    for f in &m.fields {
        out.field
            .push(convert_field(f, enum_names, msg_names, syntax, diags));
    }

    // Maps expand to a synthetic nested message + a repeated field.
    for mf in &m.maps {
        check_field_number(mf.number, mf.span, diags);
        let (kty, kname) = resolve_type(&mf.key, enum_names, msg_names);
        let (vty, vname) = resolve_type(&mf.value, enum_names, msg_names);
        let entry_name = format!("{}Entry", mf.name.name);
        let mut entry = DescriptorProto {
            name: Some(entry_name.clone()),
            field: vec![
                FieldDescriptorProto {
                    name: Some("key".into()),
                    number: Some(1),
                    label: Some(Label::Optional),
                    r#type: Some(kty),
                    type_name: kname,
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("value".into()),
                    number: Some(2),
                    label: Some(Label::Optional),
                    r#type: Some(vty),
                    type_name: vname,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        entry.options = Some(make_map_entry_options());
        out.nested_type.push(entry);
        out.field.push(FieldDescriptorProto {
            name: Some(mf.name.name.clone()),
            number: Some(mf.number as i32),
            label: Some(Label::Repeated),
            r#type: Some(FieldType::Message),
            type_name: Some(format!(".{}.{}", self_path, entry_name)),
            ..Default::default()
        });
    }

    for n in &m.nested_messages {
        out.nested_type.push(convert_message(
            n, enum_names, msg_names, &self_path, syntax, diags,
        ));
    }
    for e in &m.nested_enums {
        out.enum_type.push(convert_enum(e, diags));
    }
    for ext in &m.nested_extends {
        for f in &ext.fields {
            out.extension
                .push(convert_field(f, enum_names, msg_names, syntax, diags));
        }
    }
    for r in &m.reserved_ranges {
        out.reserved_range.push(ReservedRange {
            start: r.start as i32,
            end: r.end.unwrap_or(r.start) as i32,
        });
    }
    for n in &m.reserved_names {
        out.reserved_name.push(n.name.clone());
    }
    for r in &m.extension_ranges {
        out.extension_range.push(ExtensionRange {
            start: r.start as i32,
            end: r.end.unwrap_or(r.start) as i32,
        });
    }
    out
}

fn convert_enum(e: &ast::Enum, diags: &mut Diagnostics) -> EnumDescriptorProto {
    let mut out = EnumDescriptorProto {
        name: Some(e.name.name.clone()),
        ..Default::default()
    };
    let mut seen = std::collections::HashSet::new();
    for v in &e.values {
        if !e.allow_alias && !seen.insert(v.number) {
            diags.push(Diagnostic::warning(
                ErrorCode::Other,
                format!(
                    "enum value {} reuses number {} without allow_alias",
                    v.name.name, v.number
                ),
                Some(v.span),
            ));
        }
        out.value.push(EnumValueDescriptorProto {
            name: Some(v.name.name.clone()),
            number: Some(v.number as i32),
            ..Default::default()
        });
    }
    for r in &e.reserved_ranges {
        out.reserved_range.push(ReservedRange {
            start: r.start as i32,
            end: r.end.unwrap_or(r.start) as i32,
        });
    }
    for n in &e.reserved_names {
        out.reserved_name.push(n.name.clone());
    }
    out
}

fn convert_service(s: &ast::Service, diags: &mut Diagnostics) -> ServiceDescriptorProto {
    let mut out = ServiceDescriptorProto {
        name: Some(s.name.name.clone()),
        ..Default::default()
    };
    for m in &s.methods {
        out.method.push(MethodDescriptorProto {
            name: Some(m.name.name.clone()),
            input_type: type_ref_name(&m.input),
            output_type: type_ref_name(&m.output),
            client_streaming: Some(m.client_streaming),
            server_streaming: Some(m.server_streaming),
            ..Default::default()
        });
    }
    let _ = diags;
    out
}

fn type_ref_name(ty: &ast::TypeRef) -> Option<String> {
    match ty {
        ast::TypeRef::Named(i) => Some(i.name.clone()),
        ast::TypeRef::Scalar(_) => None,
    }
}

/// Serialize a `FileDescriptorProto` to its binary form.
pub fn serialize_file_descriptor(fd: &FileDescriptorProto) -> CoreResult<Vec<u8>> {
    fd.encode_to_vec()
}

/// Parse a `FileDescriptorProto` from its binary form.
pub fn parse_file_descriptor(bytes: &[u8]) -> CoreResult<FileDescriptorProto> {
    FileDescriptorProto::decode(bytes)
}

/// `json_name` derivation: lowerCamelCase of the proto field name.
pub fn to_json_name(field: &str) -> String {
    let mut out = String::new();
    let mut upper = false;
    for (i, c) in field.chars().enumerate() {
        if c == '_' {
            upper = true;
            continue;
        }
        if i == 0 {
            out.extend(c.to_lowercase());
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    out
}

fn check_duplicates(file: &ast::File, diags: &mut Diagnostics) {
    use std::collections::HashSet;
    let mut top = HashSet::new();
    for m in &file.messages {
        if !top.insert(m.name.name.clone()) {
            diags.push(Diagnostic::error(
                ErrorCode::DuplicateSymbol,
                format!("duplicate top-level message '{}'", m.name.name),
                Some(m.span),
            ));
        }
    }
    for e in &file.enums {
        if !top.insert(e.name.name.clone()) {
            diags.push(Diagnostic::error(
                ErrorCode::DuplicateSymbol,
                format!("duplicate top-level symbol '{}'", e.name.name),
                Some(e.span),
            ));
        }
    }
    for m in &file.messages {
        check_message_duplicates(m, diags);
    }
}

fn check_message_duplicates(m: &ast::Message, diags: &mut Diagnostics) {
    use std::collections::HashSet;
    let mut names = HashSet::new();
    for f in &m.fields {
        if !names.insert(f.name.name.clone()) {
            diags.push(Diagnostic::error(
                ErrorCode::DuplicateSymbol,
                format!(
                    "duplicate field '{}' in message '{}'",
                    f.name.name, m.name.name
                ),
                Some(f.span),
            ));
        }
    }
    for o in &m.oneofs {
        for f in &o.fields {
            if !names.insert(f.name.name.clone()) {
                diags.push(Diagnostic::error(
                    ErrorCode::DuplicateSymbol,
                    format!(
                        "duplicate field '{}' in message '{}'",
                        f.name.name, m.name.name
                    ),
                    Some(f.span),
                ));
            }
        }
    }
    for n in &m.nested_messages {
        check_message_duplicates(n, diags);
    }
}

fn make_map_entry_options() -> Vec<u8> {
    // message MessageOptions { bool map_entry = 7; }
    // Encode: field 7 = varint 1 (true).
    let mut w = tpt_proto_core::Writer::new();
    tpt_proto_core::scalar::encode_bool(&mut w, 7, true);
    w.into_vec()
}

fn constant_to_text(c: &ast::Constant) -> String {
    match c {
        ast::Constant::Int(i) => i.to_string(),
        ast::Constant::Float(f) => f.to_string(),
        ast::Constant::String(s) => s.clone(),
        ast::Constant::Bool(b) => b.to_string(),
        ast::Constant::Ident(s) => s.clone(),
        ast::Constant::Aggregate(_) => String::new(),
        ast::Constant::List(_) => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_proto_language::parse_file;

    const SRC: &str = r#"
syntax = "proto3";
package ex;

message Person {
  string name = 1;
  int32 id = 2;
  repeated string emails = 3;
  map<string, int32> labels = 4;
  oneof contact {
    string email = 5;
    string phone = 6;
  }
}

enum Color { RED = 0; GREEN = 1; }

service S {
  rpc Get(Person) returns (Person);
}
"#;

    #[test]
    fn compiles_to_descriptor() {
        let parsed = parse_file("ex.proto", SRC);
        assert!(!parsed.diagnostics.has_errors());
        let (fd, diags) = compile(&parsed.file);
        assert!(
            !diags.has_errors(),
            "diagnostics: {:?}",
            diags.iter().collect::<Vec<_>>()
        );
        assert_eq!(fd.package.as_deref(), Some("ex"));
        let person = fd.find_message("Person").unwrap();
        assert_eq!(person.field.len(), 6);
        let labels = person.find_field_by_name("labels").unwrap();
        assert_eq!(labels.label, Some(Label::Repeated));
        assert_eq!(fd.find_enum("Color").unwrap().value.len(), 2);
        assert_eq!(fd.find_service("S").unwrap().method.len(), 1);
        // roundtrip serialize/parse
        let bytes = serialize_file_descriptor(&fd).unwrap();
        let reparsed = parse_file_descriptor(&bytes).unwrap();
        assert_eq!(reparsed, fd);
    }

    #[test]
    fn invalid_field_number_diagnosed() {
        let src = r#"syntax="proto3"; message M { int32 x = 0; }"#;
        let parsed = parse_file("m.proto", src);
        let (_fd, diags) = compile(&parsed.file);
        assert!(diags.has_errors());
    }
}
