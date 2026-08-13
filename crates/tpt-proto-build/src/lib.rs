//! `tpt-proto-build` — `build.rs` integration for compiling `.proto` files (§4.12).
//!
//! Provides [`compile_protos`] which drives the tpt-proto parser, compiler, and
//! Rust code generator, emitting one `.rs` module per input `.proto` into the
//! configured output directory. Designed to be called from a crate's
//! `build.rs`.
//!
//! Typical usage in `build.rs`:
//!
//! ```no_run
//! fn main() -> std::io::Result<()> {
//!     tpt_proto_build::compile_protos(
//!         &["proto/service.proto".into()],
//!         &["proto".into()],
//!         &std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()),
//!     ).expect("protobuf compilation failed");
//!     Ok(())
//! }
//! ```

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use tpt_proto_codegen_rust::{generate, GenerateOptions};
use tpt_proto_compiler::compile;
use tpt_proto_descriptor::FileDescriptorSet;
use tpt_proto_language::{parse_file, Diagnostic};

/// Recursively collect `.proto` files under `root`.
fn collect_protos(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if root.is_file() {
        if root.extension().map(|e| e == "proto").unwrap_or(false) {
            out.push(root.to_path_buf());
        }
        return Ok(out);
    }
    let entries = std::fs::read_dir(root).with_context(|| format!("reading dir {}", root.display()))?;
    for e in entries {
        let p = e?.path();
        if p.is_dir() {
            out.extend(collect_protos(&p)?);
        } else if p.extension().map(|e| e == "proto").unwrap_or(false) {
            out.push(p);
        }
    }
    Ok(out)
}

/// Compile the given `.proto` files (with `includes` as import search paths)
/// and emit generated Rust modules into `out_dir`.
///
/// Emits `<module>.rs` for each `.proto`, plus a generated `mod.rs` listing
/// them so they can be included via `include!(concat!(env!("OUT_DIR"), "/mod.rs"))`.
pub fn compile_protos(protos: &[PathBuf], includes: &[PathBuf], out_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    let mut modules: Vec<String> = Vec::new();

    // Expand directories into concrete file lists.
    let mut files = Vec::new();
    for p in protos {
        files.extend(collect_protos(p)?);
    }

    for path in &files {
        let code = compile_one(path, includes)?;
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .context("invalid proto file name")?
            .replace(['-', '.'], "_");
        let out_path = out_dir.join(format!("{stem}.rs"));
        std::fs::write(&out_path, &code).with_context(|| format!("writing {}", out_path.display()))?;
        modules.push(stem);
    }

    let mut mod_rs = String::new();
    for m in &modules {
        mod_rs.push_str(&format!("pub mod {m};\n"));
    }
    std::fs::write(out_dir.join("mod.rs"), mod_rs).with_context(|| format!("writing {}/mod.rs", out_dir.display()))?;
    Ok(())
}

/// Compile a single `.proto` file into generated Rust source.
fn compile_one(path: &Path, includes: &[PathBuf]) -> Result<String> {
    let src = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("input.proto")
        .to_string();

    // Provide include-path contents as additional parsed files so imports
    // resolve during compilation.
    let mut parsed = parse_file(&name, &src);
    let mut diags: Vec<Diagnostic> = parsed.diagnostics.iter().cloned().collect();
    for inc in includes {
        for dep in collect_protos(inc)? {
            let dsrc = std::fs::read_to_string(&dep).with_context(|| format!("reading {}", dep.display()))?;
            let dname = dep.file_name().and_then(|s| s.to_str()).unwrap_or("dep.proto").to_string();
            let dparsed = parse_file(&dname, &dsrc);
            diags.extend(dparsed.diagnostics.iter().cloned());
            // Merge dependency messages into the same parsed file root so the
            // compiler can see them.
            parsed.file.messages.extend(dparsed.file.messages);
            parsed.file.enums.extend(dparsed.file.enums);
        }
    }
    if diags.iter().any(|d| d.severity == tpt_proto_language::Severity::Error) {
        for d in &diags {
            eprintln!("{}: {}", severity_str(d), d.message);
        }
        bail!("compilation of {} failed", path.display());
    }
    let (fd, _cdiags) = compile(&parsed.file);
    let set = FileDescriptorSet { file: vec![fd] };
    let opts = GenerateOptions::default();
    generate(&set, &opts).map_err(|e| anyhow::anyhow!("codegen error in {}: {e}", path.display()))
}

fn severity_str(d: &Diagnostic) -> &'static str {
    match d.severity {
        tpt_proto_language::Severity::Error => "error",
        tpt_proto_language::Severity::Warning => "warning",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_proto_files() {
        let tmp = std::env::temp_dir().join("tpt_build_test");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("a.proto"), "syntax = \"proto3\"; package t; message A {}").unwrap();
        std::fs::write(tmp.join("b.proto"), "syntax = \"proto3\"; package t; message B {}").unwrap();
        let mut files = collect_protos(&tmp).unwrap();
        files.sort();
        assert_eq!(files.len(), 2);
    }
}
