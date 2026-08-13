//! `tpt-proto-cli` — command-line interface for the tpt-proto toolchain.
//!
//! Provides `compile`, `decode`, `generate`, `descriptors`, `json`, `text`,
//! and conversion commands that drive the language parser, compiler,
//! reflection, codegen, JSON, and text layers.

use std::path::Path;
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use tpt_proto_codegen_rust::{generate, GenerateOptions};
use tpt_proto_compiler::{compile, serialize_file_descriptor};
use tpt_proto_core::{Message, Reader};
use tpt_proto_descriptor::{DescriptorProto, FileDescriptorProto, FileDescriptorSet};
use tpt_proto_json::{json_string_to_message, message_to_json_string, JsonOptions};
use tpt_proto_language::{parse_file, Diagnostic};
use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue, Value};
use tpt_proto_lint::lint as run_lint;
use tpt_proto_text::{message_to_text, text_to_message, TextOptions};


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

// ---------------------------------------------------------------------------
// Higher-level command helpers.
// ---------------------------------------------------------------------------

/// Compile a `.proto` file into a full [`FileDescriptorSet`].
pub fn compile_set(path: &Path) -> Result<FileDescriptorSet> {
    let (fd, diags) = compile_path(path)?;
    report_diagnostics(&diags)?;
    Ok(FileDescriptorSet { file: vec![fd] })
}

/// Generate Rust source code for a `.proto` file.
pub fn generate_code(path: &Path, grpc: bool) -> Result<String> {
    let set = compile_set(path)?;
    let opts = GenerateOptions {
        grpc,
        ..Default::default()
    };
    generate(&set, &opts).map_err(|e| anyhow::anyhow!("codegen error: {e}"))
}

/// Serialize a `.proto` file's descriptor set to bytes.
pub fn descriptor_set_bytes(path: &Path) -> Result<Vec<u8>> {
    let set = compile_set(path)?;
    set.encode_to_vec().context("encoding descriptor set")
}

/// Read a binary file as bytes.
pub fn read_bytes(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("reading {}", path.display()))
}

/// Resolve a message descriptor (by simple or fully-qualified name) from a
/// compiled `.proto` file.
pub fn lookup_message(
    pool: &DescriptorPool,
    fd: &FileDescriptorProto,
    message: &str,
) -> Result<Arc<DescriptorProto>> {
    let pkg = fd.package.as_deref().unwrap_or("");
    pool.lookup_message(message)
        .or_else(|| pool.lookup_message(&format!(".{pkg}.{message}")))
        .or_else(|| pool.lookup_message(&format!("{pkg}.{message}")))
        .context(format!("message '{message}' not found"))
}

/// Decode binary wire bytes for a named message.
pub fn decode_message(
    pool: &DescriptorPool,
    desc: &Arc<DescriptorProto>,
    bytes: &[u8],
) -> Result<DynamicMessage> {
    let mut r = Reader::new(bytes);
    Ok(DynamicMessage::decode(pool, desc.clone(), &mut r)?)
}

/// Convert binary wire bytes into a JSON string for the named message.
pub fn binary_to_json(pool: &DescriptorPool, desc: &Arc<DescriptorProto>, bytes: &[u8]) -> Result<String> {
    let dm = decode_message(pool, desc, bytes)?;
    let opts = JsonOptions::default();
    Ok(message_to_json_string(pool, desc, &dm, &opts)?)
}

/// Convert a JSON string into binary wire bytes for the named message.
pub fn json_to_binary(pool: &DescriptorPool, desc: &Arc<DescriptorProto>, json: &str) -> Result<Vec<u8>> {
    let opts = JsonOptions::default();
    let dm = json_string_to_message(pool, desc, json, &opts)?;
    dm.encode().context("encoding message")
}

/// Convert binary wire bytes into text format for the named message.
pub fn binary_to_text(pool: &DescriptorPool, desc: &Arc<DescriptorProto>, bytes: &[u8]) -> Result<String> {
    let dm = decode_message(pool, desc, bytes)?;
    let opts = TextOptions::default();
    Ok(message_to_text(pool, desc, &dm, &opts))
}

/// Convert text format into binary wire bytes for the named message.
pub fn text_to_binary(pool: &DescriptorPool, desc: &Arc<DescriptorProto>, text: &str) -> Result<Vec<u8>> {
    let opts = TextOptions::default();
    let dm = text_to_message(pool, desc, text, &opts)?;
    dm.encode().context("encoding message")
}

/// Lint the evolution from `old_proto` to `new_proto`.
pub fn lint_files(old_proto: &Path, new_proto: &Path, json: bool) -> Result<String> {
    let old_set = compile_set(old_proto)?;
    let new_set = compile_set(new_proto)?;
    let old_fd = old_set.file.first().context("old descriptor set empty")?;
    let new_fd = new_set.file.first().context("new descriptor set empty")?;
    let report = run_lint(old_fd, new_fd);
    Ok(if json { report.render_json() } else { report.render() })
}
/// Produce a textual diff summary between two descriptor-set binaries.
pub fn diff_descriptors(a: &[u8], b: &[u8]) -> Result<String> {
    let sa = FileDescriptorSet::decode(a).context("decoding first descriptor set")?;
    let sb = FileDescriptorSet::decode(b).context("decoding second descriptor set")?;
    let names_a: Vec<String> = sa.file.iter().flat_map(|f| f.message_type.iter().map(|m| m.name.clone().unwrap_or_default())).collect();
    let names_b: Vec<String> = sb.file.iter().flat_map(|f| f.message_type.iter().map(|m| m.name.clone().unwrap_or_default())).collect();
    let mut out = String::new();
    for n in &names_a {
        if !names_b.contains(n) {
            out.push_str(&format!("- {n}\n"));
        }
    }
    for n in &names_b {
        if !names_a.contains(n) {
            out.push_str(&format!("+ {n}\n"));
        }
    }
    if out.is_empty() {
        out.push_str("(no message-name differences)\n");
    }
    Ok(out)
}
