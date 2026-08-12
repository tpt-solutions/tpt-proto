//! `tpt-proto-cli` — command-line interface for the tpt-proto toolchain.
//!
//! Provides `compile`, `decode`, and `describe` commands that drive the
//! language parser, compiler, and reflection runtime.

use std::path::Path;

use anyhow::{bail, Context, Result};
use tpt_proto_compiler::{compile, serialize_file_descriptor};
use tpt_proto_core::Reader;
use tpt_proto_descriptor::{DescriptorProto, FileDescriptorProto};
use tpt_proto_language::{parse_file, Diagnostic};
use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue, Value};

/// Parse and compile a `.proto` source file into a [`FileDescriptorProto`].
pub fn compile_path(path: &Path) -> Result<(FileDescriptorProto, Vec<Diagnostic>)> {
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("input.proto")
        .to_string();
    let parsed = parse_file(&name, &src);
    let diags: Vec<Diagnostic> = parsed.diagnostics.iter().cloned().collect();
    let (fd, compile_diags) = compile(&parsed.file);
    let mut all = diags;
    all.extend(compile_diags.iter().cloned());
    Ok((fd, all))
}

/// Decode a hex string as the named message from a compiled `.proto` file.
pub fn decode_hex(proto_path: &Path, message: &str, hex: &str) -> Result<DynamicMessage> {
    let (fd, diags) = compile_path(proto_path)?;
    report_diagnostics(&diags)?;
    let pool = DescriptorPool::from_file(&fd);
    let desc = pool
        .lookup_message(message)
        .or_else(|| pool.lookup_message(&format!("{}.{}", fd.package.as_deref().unwrap_or(""), message)))
        .context(format!("message '{message}' not found"))?;
    let bytes = hex::decode(hex.trim())?;
    let mut r = Reader::new(&bytes);
    let dm = DynamicMessage::decode(&pool, desc, &mut r)?;
    Ok(dm)
}

fn report_diagnostics(diags: &[Diagnostic]) -> Result<()> {
    for d in diags {
        eprintln!("{}: {}", severity_str(d), d.message);
    }
    if diags.iter().any(|d| d.severity == tpt_proto_language::Severity::Error) {
        bail!("compilation failed with errors");
    }
    Ok(())
}

fn severity_str(d: &Diagnostic) -> &'static str {
    match d.severity {
        tpt_proto_language::Severity::Error => "error",
        tpt_proto_language::Severity::Warning => "warning",
    }
}

/// Pretty-print a dynamic message (best-effort, human readable).
pub fn print_message(dm: &DynamicMessage, indent: usize) {
    let pad = "  ".repeat(indent);
    for (num, value) in &dm.fields {
        print!("{pad}{num}: ");
        print_value(value, indent);
    }
    if !dm.unknown.is_empty() {
        println!("{pad}[[unknown fields: {}]]", dm.unknown.len());
    }
}

fn print_value(value: &Value, indent: usize) {
    match value {
        Value::Scalar(s) => {
            println!("{}", scalar_repr(s));
        }
        Value::Enum(e) => println!("(enum) {e}"),
        Value::Message(m) => {
            println!("{{");
            print_message(m, indent + 1);
            println!("{}  }}", "  ".repeat(indent));
        }
        Value::List(items) => {
            println!("[");
            for it in items {
                print!("{}{}", "  ".repeat(indent + 1), "");
                print_value(it, indent + 1);
            }
            println!("{}  ]", "  ".repeat(indent));
        }
        Value::Map(entries) => {
            println!("{{");
            for (k, v) in entries {
                println!("{}{} => {}", "  ".repeat(indent + 1), scalar_repr_opt(k), scalar_repr_opt(v));
            }
            println!("{}  }}", "  ".repeat(indent));
        }
    }
}

fn scalar_repr_opt(v: &Value) -> String {
    match v {
        Value::Scalar(s) => scalar_repr(s),
        Value::Enum(e) => format!("(enum) {e}"),
        _ => "...".to_string(),
    }
}

fn scalar_repr(s: &ScalarValue) -> String {
    match s {
        ScalarValue::I64(v) => v.to_string(),
        ScalarValue::U64(v) => v.to_string(),
        ScalarValue::F64(v) => v.to_string(),
        ScalarValue::Bool(b) => b.to_string(),
        ScalarValue::String(s) => format!("{s:?}"),
        ScalarValue::Bytes(b) => format!("0x{}", hex::encode(b)),
    }
}

/// Print a structural description of the compiled file.
pub fn describe(fd: &FileDescriptorProto) {
    println!("package: {}", fd.package.as_deref().unwrap_or("(none)"));
    println!("syntax:  {}", fd.syntax.as_deref().unwrap_or("(none)"));
    println!("messages: {}", fd.message_type.len());
    for m in &fd.message_type {
        describe_message(m, 1);
    }
    println!("enums: {}", fd.enum_type.len());
    for e in &fd.enum_type {
        println!("  enum {}", e.name.as_deref().unwrap_or("?"));
    }
    println!("services: {}", fd.service.len());
    for s in &fd.service {
        println!("  service {}", s.name.as_deref().unwrap_or("?"));
    }
}

fn describe_message(m: &DescriptorProto, depth: usize) {
    let pad = "  ".repeat(depth);
    println!("{pad}message {}", m.name.as_deref().unwrap_or("?"));
    for f in &m.field {
        println!(
            "{pad}  {} {} = {}",
            f.label.map(label_name).unwrap_or("?"),
            f.name.as_deref().unwrap_or("?"),
            f.number.unwrap_or(0)
        );
    }
    for n in &m.nested_type {
        describe_message(n, depth + 1);
    }
    for e in &m.enum_type {
        println!("{pad}  enum {}", e.name.as_deref().unwrap_or("?"));
    }
}

fn label_name(l: tpt_proto_descriptor::Label) -> &'static str {
    match l {
        tpt_proto_descriptor::Label::Optional => "optional",
        tpt_proto_descriptor::Label::Required => "required",
        tpt_proto_descriptor::Label::Repeated => "repeated",
    }
}

/// Write the serialized FileDescriptorSet (this file) to `out`.
pub fn emit_descriptor_bin(fd: &FileDescriptorProto, out: &Path) -> Result<()> {
    let bytes = serialize_file_descriptor(fd)?;
    std::fs::write(out, bytes).with_context(|| format!("writing {}", out.display()))?;
    Ok(())
}
