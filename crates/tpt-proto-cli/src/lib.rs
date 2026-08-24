//! `tpt-proto-cli` — command-line interface for the tpt-proto toolchain.
//!
//! Provides `compile`, `decode`, `generate`, `descriptors`, `json`, `text`,
//! and conversion commands that drive the language parser, compiler,
//! reflection, codegen, JSON, and text layers.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub mod config;

use anyhow::{bail, Context, Result};
use tpt_proto_codegen_rust::{generate, GenerateOptions};
use tpt_proto_compiler::{compile, resolve_and_parse, serialize_file_descriptor};
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

/// Compile a `.proto` file and its transitive imports into a full
/// [`FileDescriptorSet`] through the shared import resolver and the compiler's
/// multi-file pipeline.
///
/// `includes` are the `-I`-style search directories: each `import` statement's
/// path is looked up relative to them, in order.
pub fn compile_set_with_includes(entry: &Path, includes: &[PathBuf]) -> Result<FileDescriptorSet> {
    let resolved = resolve_and_parse(entry, includes)
        .with_context(|| format!("resolving imports of {}", entry.display()))?;
    // Surface parser problems from every resolved file first.
    report_diagnostics(&resolved.parse_diagnostics())?;
    let result = resolved.compile();
    // Then semantic (pipeline) diagnostics.
    report_diagnostics(&result.diagnostics.iter().cloned().collect::<Vec<_>>())?;
    Ok(result.set)
}

/// Load a [`DescriptorPool`] covering `entry` plus all transitive imports,
/// returning the pool together with the entry file's own descriptor (useful
/// for package-relative message lookups).
pub fn load_pool(
    entry: &Path,
    includes: &[PathBuf],
) -> Result<(DescriptorPool, FileDescriptorProto)> {
    let set = compile_set_with_includes(entry, includes)?;
    let pool = DescriptorPool::from_set(&set);
    let entry_fd = set
        .file
        .last()
        .cloned()
        .context("resolved descriptor set is empty")?;
    Ok((pool, entry_fd))
}

/// Generate Rust source code for a `.proto` file (and everything it imports).
pub fn generate_code(path: &Path, includes: &[PathBuf], grpc: bool) -> Result<String> {
    let set = compile_set_with_includes(path, includes)?;
    let opts = GenerateOptions {
        grpc,
        ..Default::default()
    };
    generate(&set, &opts).map_err(|e| anyhow::anyhow!("codegen error: {e}"))
}

/// Serialize a `.proto` file's (transitively complete) descriptor set to bytes.
pub fn descriptor_set_bytes(path: &Path, includes: &[PathBuf]) -> Result<Vec<u8>> {
    let set = compile_set_with_includes(path, includes)?;
    set.encode_to_vec().context("encoding descriptor set")
}

/// Decode a hex string as the named message from a compiled `.proto` file
/// (imports resolved through `includes`).
pub fn decode_hex(
    proto_path: &Path,
    includes: &[PathBuf],
    message: &str,
    hex: &str,
) -> Result<DynamicMessage> {
    let (pool, entry_fd) = load_pool(proto_path, includes)?;
    let desc = lookup_message(&pool, &entry_fd, message)?;
    let bytes = hex::decode(hex.trim())?;
    let mut r = Reader::new(&bytes);
    let dm = DynamicMessage::decode(&pool, desc, &mut r)?;
    Ok(dm)
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

/// Apply `[lint.severity]` overrides to a report: remap severities
/// (case-insensitive rule codes) or drop findings mapped to `"ignore"`.
///
/// Unknown actions are warned about on stderr and left unchanged so a typo in
/// the config cannot silently hide findings.
pub fn apply_severity_overrides(
    report: &mut tpt_proto_lint::Report,
    overrides: &HashMap<String, String>,
) {
    use tpt_proto_lint::Severity;
    if overrides.is_empty() {
        return;
    }
    report.findings.retain_mut(|f| {
        let action = overrides
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(f.code))
            .map(|(_, v)| v.as_str());
        match action {
            None => true,
            Some("error") => {
                f.severity = Severity::Error;
                true
            }
            Some("warning") => {
                f.severity = Severity::Warning;
                true
            }
            Some("ignore") => false,
            Some(other) => {
                eprintln!(
                    "warning: unknown severity `{other}` for lint override `{}` (expected error|warning|ignore)",
                    f.code
                );
                true
            }
        }
    });
}

/// Lint the evolution from `old_proto` to `new_proto` (imports resolved via
/// `includes`; `severity_overrides` applied before rendering); each side's
/// *entry* file descriptor is compared.
pub fn lint_files(
    old_proto: &Path,
    new_proto: &Path,
    json: bool,
    includes: &[PathBuf],
    severity_overrides: &HashMap<String, String>,
) -> Result<String> {
    let old_set = compile_set_with_includes(old_proto, includes)?;
    let new_set = compile_set_with_includes(new_proto, includes)?;
    let old_fd = old_set.file.last().context("old descriptor set empty")?;
    let new_fd = new_set.file.last().context("new descriptor set empty")?;
    let mut report = run_lint(old_fd, new_fd);
    apply_severity_overrides(&mut report, severity_overrides);
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

/// Scaffold a new tpt-proto starter crate at `<root>/<name>`.
///
/// Generates a minimal but fully working project: a `Cargo.toml` wired for
/// `tpt-proto-core` + `tpt-proto-build`, a `build.rs` that compiles
/// `proto/app.proto` into `$OUT_DIR`, and a `src/main.rs` demonstrating an
/// encode/decode roundtrip. Returns the created directory path.
///
/// Fails unless `force` is set when the target directory already exists.
pub fn init_project(name: &str, force: bool) -> Result<PathBuf> {
    let valid = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !name.starts_with('-');
    if !valid {
        bail!("invalid crate name `{name}`: use ASCII letters, digits, `-`, `_`");
    }

    let root = PathBuf::from(name);
    if root.exists() && !force {
        bail!("directory `{}` already exists (pass --force to overwrite)", root.display());
    }
    std::fs::create_dir_all(root.join("src"))?;
    std::fs::create_dir_all(root.join("proto"))?;

    let cargo_toml = format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
tpt-proto-core = "0"

[build-dependencies]
tpt-proto-build = "0"
"#
    );
    let build_rs = r#"fn main() {
    tpt_proto_build::compile_protos_simple(
        &["proto/app.proto".into()],
        &["proto".into()],
    )
    .expect("protobuf compilation failed");
}
"#;
    let app_proto = r#"syntax = "proto3";

package app;

message Item {
  int64 id = 1;
  string name = 2;
}
"#;
    let main_rs = r#"//! Generated message types live in $OUT_DIR; include them here.
include!(concat!(env!("OUT_DIR"), "/app.rs"));

fn main() {
    let item = Item { id: 1, name: "first".into(), ..Default::default() };
    let bytes = item.encode_to_vec().expect("encode");
    let back = Item::decode(&bytes).expect("decode");
    assert_eq!(item.id, back.id);
    println!("{back:?}");
}
"#;

    std::fs::write(root.join("Cargo.toml"), cargo_toml)?;
    std::fs::write(root.join("build.rs"), build_rs)?;
    std::fs::write(root.join("proto").join("app.proto"), app_proto)?;
    std::fs::write(root.join("src").join("main.rs"), main_rs)?;
    std::fs::write(root.join(".gitignore"), "/target\n")?;
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_scaffolds_working_layout() {
        let base = std::env::temp_dir().join(format!("tpt_init_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let prev = std::env::current_dir().unwrap();
        std::env::set_current_dir(&base).unwrap();

        let path = init_project("demo", false).unwrap();
        assert!(path.join("Cargo.toml").exists());
        assert!(path.join("build.rs").exists());
        assert!(path.join("proto").join("app.proto").exists());
        assert!(path.join("src").join("main.rs").exists());

        // Existing dir without --force must fail.
        assert!(init_project("demo", false).is_err());
        // With --force it succeeds and overwrites.
        assert!(init_project("demo", true).is_ok());

        // Invalid names are rejected.
        assert!(init_project("", false).is_err());
        assert!(init_project("has space", false).is_err());

        std::env::set_current_dir(prev).unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }
}
