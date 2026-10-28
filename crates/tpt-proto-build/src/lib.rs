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
//! // Simplest form: infers $OUT_DIR and uses default configuration.
//! fn main() -> std::io::Result<()> {
//!     tpt_proto_build::compile_protos_simple(
//!         &["proto/service.proto".into()],
//!         &["proto".into()],
//!     ).expect("protobuf compilation failed");
//!     Ok(())
//! }
//! ```
//!
//! For gRPC code generation or incremental rebuilds, use [`compile_protos`]
//! with an explicit [`BuildConfig`]:
//!
//! ```no_run
//! fn main() -> std::io::Result<()> {
//!     let config = tpt_proto_build::BuildConfig::grpc();
//!     tpt_proto_build::compile_protos(
//!         &["proto/service.proto".into()],
//!         &["proto".into()],
//!         &std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()),
//!         &config,
//!     ).expect("protobuf compilation failed");
//!     Ok(())
//! }
//! ```
//!
//! Both entry points emit `cargo:rerun-if-changed` directives for every input
//! `.proto`, every included dependency, and every include directory, so Cargo
//! automatically re-runs the build script whenever any schema input changes.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use tpt_proto_codegen_rust::{generate, GenerateOptions};
use tpt_proto_compiler::resolve_and_parse;
use tpt_proto_language::Diagnostic;

/// Configuration for [`compile_protos`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct BuildConfig {
    /// Emit async gRPC server traits and client stubs for `service` blocks.
    pub grpc: bool,
    /// Wrap each file's types in a package-named module.
    pub module_per_package: bool,
    /// Only (re)generate a `.rs` file when its source `.proto` (or any included
    /// dependency) actually changed since the previous build. Skipped files are
    /// not rewritten, avoiding unnecessary downstream recompilation.
    pub incremental: bool,
}

impl BuildConfig {
    /// A configuration enabling gRPC code generation.
    pub fn grpc() -> BuildConfig {
        BuildConfig {
            grpc: true,
            ..Default::default()
        }
    }
}

/// A stable fingerprint of everything that affects a generated file: the source
/// file contents, every included `.proto`, and the build configuration.
fn fingerprint(path: &Path, includes: &[PathBuf], config: &BuildConfig) -> u64 {
    let mut hasher = DefaultHasher::new();
    if let Ok(src) = std::fs::read(path) {
        src.hash(&mut hasher);
    }
    for inc in includes {
        for dep in collect_protos(inc).unwrap_or_default() {
            std::fs::read(&dep).unwrap_or_default().hash(&mut hasher);
        }
    }
    config.hash(&mut hasher);
    hasher.finish()
}

/// Recursively collect `.proto` files under `root`.
fn collect_protos(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if root.is_file() {
        if root.extension().map(|e| e == "proto").unwrap_or(false) {
            out.push(root.to_path_buf());
        }
        return Ok(out);
    }
    let entries =
        std::fs::read_dir(root).with_context(|| format!("reading dir {}", root.display()))?;
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
/// and emit generated Rust modules into `$OUT_DIR` using default configuration.
///
/// This is the low-boilerplate equivalent of:
///
/// ```no_run
/// # fn main() -> anyhow::Result<()> {
/// let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR")?);
/// tpt_proto_build::compile_protos(
///     &[],
///     &[],
///     &out_dir,
///     &tpt_proto_build::BuildConfig::default(),
/// )?;
/// # Ok(())
/// # }
/// ```
///
/// Must be called from a `build.rs` (it reads the `OUT_DIR` environment
/// variable set by Cargo). To enable gRPC stubs, incremental regeneration, or
/// package-module wrapping, call [`compile_protos`] with a custom
/// [`BuildConfig`] instead.
pub fn compile_protos_simple(protos: &[PathBuf], includes: &[PathBuf]) -> Result<()> {
    let out_dir = std::env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "`OUT_DIR` is not set: compile_protos_simple must be called from build.rs"
            )
        })?;
    compile_protos(protos, includes, &out_dir, &BuildConfig::default())
}

/// Tell Cargo when the build script must re-run: whenever any input `.proto`
/// (as passed *or* as expanded from a directory), any imported dependency, or
/// any include directory changes. Pointing at the include directories also
/// covers files added to them later.
///
/// Emitted paths are relative-friendly: build scripts run with the crate
/// manifest directory as CWD, so the raw paths Cargo receives resolve correctly.
fn emit_rerun_if_changed(protos: &[PathBuf], includes: &[PathBuf], files: &[PathBuf]) {
    // Only meaningful inside a build script; skipped elsewhere so plain library
    // calls (and unit tests) don't print spurious cargo directives.
    if std::env::var_os("OUT_DIR").is_none() {
        return;
    }
    let mut seen = std::collections::HashSet::new();
    let mut emit = |p: &Path| {
        if seen.insert(p.to_path_buf()) {
            println!("cargo:rerun-if-changed={}", p.display());
        }
    };
    for p in protos {
        emit(p);
    }
    for inc in includes {
        emit(inc);
        for dep in collect_protos(inc).unwrap_or_default() {
            emit(&dep);
        }
    }
    for f in files {
        emit(f);
    }
}

/// Compile the given `.proto` files (with `includes` as import search paths)
/// and emit generated Rust modules into `out_dir`.
///
/// Emits `<module>.rs` for each `.proto`, plus a generated `mod.rs` listing
/// them so they can be included via `include!(concat!(env!("OUT_DIR"), "/mod.rs"))`.
///
/// Behaviour is controlled by `config` (see [`BuildConfig`]).
pub fn compile_protos(
    protos: &[PathBuf],
    includes: &[PathBuf],
    out_dir: &Path,
    config: &BuildConfig,
) -> Result<()> {
    std::fs::create_dir_all(out_dir).with_context(|| format!("creating {}", out_dir.display()))?;

    let mut modules: Vec<String> = Vec::new();

    // Expand directories into concrete file lists.
    let mut files = Vec::new();
    for p in protos {
        files.extend(collect_protos(p)?);
    }

    emit_rerun_if_changed(protos, includes, &files);

    for path in &files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .context("invalid proto file name")?
            .replace(['-', '.'], "_");
        let out_path = out_dir.join(format!("{stem}.rs"));
        let stamp_path = out_dir.join(format!("{stem}.tptstamp"));
        let fp = fingerprint(path, includes, config);

        // Incremental mode: skip regeneration when inputs are unchanged.
        if config.incremental && stamp_path.exists() {
            if let Ok(prev) = std::fs::read_to_string(&stamp_path) {
                if prev.trim() == fp.to_string() && out_path.exists() {
                    modules.push(stem);
                    continue;
                }
            }
        }

        let code = compile_one(path, includes, config)?;
        std::fs::write(&out_path, &code)
            .with_context(|| format!("writing {}", out_path.display()))?;
        std::fs::write(&stamp_path, fp.to_string())
            .with_context(|| format!("writing {}", stamp_path.display()))?;
        modules.push(stem);
    }

    let mut mod_rs = String::new();
    for m in &modules {
        mod_rs.push_str(&format!("pub mod {m};\n"));
    }
    std::fs::write(out_dir.join("mod.rs"), mod_rs)
        .with_context(|| format!("writing {}/mod.rs", out_dir.display()))?;
    Ok(())
}

/// Compile a single `.proto` file into generated Rust source.
///
/// Imports are resolved through `tpt-proto-compiler`'s shared
/// [`resolve_and_parse`] resolver (the same implementation the CLI uses), and
/// the entry file plus its transitive dependencies are compiled together
/// through the full multi-file pipeline — so cross-file type references work
/// with real import semantics, without merging unrelated files that merely
/// happen to live in an include directory.
fn compile_one(path: &Path, includes: &[PathBuf], config: &BuildConfig) -> Result<String> {
    let resolved = resolve_and_parse(path, includes)
        .with_context(|| format!("resolving imports of {}", path.display()))?;
    let mut diags = resolved.parse_diagnostics();
    let compiled = resolved.compile();
    diags.extend(compiled.diagnostics.iter().cloned());
    if diags
        .iter()
        .any(|d| d.severity == tpt_proto_language::Severity::Error)
    {
        for d in &diags {
            eprintln!("{}: {}", severity_str(d), d.message);
        }
        bail!("compilation of {} failed", path.display());
    }
    let opts = GenerateOptions {
        grpc: config.grpc,
        module_per_package: config.module_per_package,
        ..GenerateOptions::default()
    };
    generate(&compiled.set, &opts)
        .map_err(|e| anyhow::anyhow!("codegen error in {}: {e}", path.display()))
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
        std::fs::write(
            tmp.join("a.proto"),
            "syntax = \"proto3\"; package t; message A {}",
        )
        .unwrap();
        std::fs::write(
            tmp.join("b.proto"),
            "syntax = \"proto3\"; package t; message B {}",
        )
        .unwrap();
        let mut files = collect_protos(&tmp).unwrap();
        files.sort();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn incremental_skips_unchanged() {
        let root = std::env::temp_dir().join("tpt_incr_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let src = root.join("m.proto");
        std::fs::write(
            &src,
            "syntax = \"proto3\"; package t; message M { int32 a = 1; }",
        )
        .unwrap();
        let out = root.join("out");
        std::fs::create_dir_all(&out).unwrap();

        let config = BuildConfig {
            incremental: true,
            ..Default::default()
        };
        compile_protos(&[src.clone()], &[], &out, &config).unwrap();
        let out_rs = out.join("m.rs");
        assert!(out_rs.exists());
        let first_mtime = std::fs::metadata(&out_rs).unwrap().modified().unwrap();

        // Second build with identical inputs should not rewrite the output.
        std::thread::sleep(std::time::Duration::from_millis(20));
        compile_protos(&[src.clone()], &[], &out, &config).unwrap();
        let second_mtime = std::fs::metadata(&out_rs).unwrap().modified().unwrap();
        assert_eq!(
            first_mtime, second_mtime,
            "output was rewritten despite no changes"
        );

        // Changing the source should force regeneration.
        std::fs::write(
            &src,
            "syntax = \"proto3\"; package t; message M { int32 a = 1; string b = 2; }",
        )
        .unwrap();
        compile_protos(&[src.clone()], &[], &out, &config).unwrap();
        let third_mtime = std::fs::metadata(&out_rs).unwrap().modified().unwrap();
        assert_ne!(
            second_mtime, third_mtime,
            "output was not regenerated after source change"
        );
    }

    #[test]
    fn simple_wrapper_requires_out_dir() {
        // Outside a build script there is no OUT_DIR, so the convenience
        // wrapper must fail with a clear error rather than panic.
        if std::env::var_os("OUT_DIR").is_some() {
            return; // running in an environment that sets OUT_DIR; skip.
        }
        let root = std::env::temp_dir().join("tpt_simple_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let src = root.join("m.proto");
        std::fs::write(
            &src,
            "syntax = \"proto3\"; package t; message M { int32 a = 1; }",
        )
        .unwrap();
        let err = compile_protos_simple(&[src], &[]).unwrap_err();
        assert!(
            err.to_string().contains("OUT_DIR"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn imports_resolve_through_shared_includes_without_spurious_merges() {
        // The entry imports a dependency from an include dir; a second,
        // completely unrelated proto also lives in that include dir. Only the
        // entry + its real dependency may contribute types to the generated
        // module (the old flat-merge hack pulled in the unrelated file too).
        let root = std::env::temp_dir().join(format!("tpt_import_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("include")).unwrap();
        std::fs::create_dir_all(root.join("out")).unwrap();

        std::fs::write(
            root.join("include/dep.proto"),
            "syntax = \"proto3\";\npackage d;\nmessage Dep { int32 x = 1; }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("include/unrelated.proto"),
            "syntax = \"proto3\";\npackage u;\nmessage Unrelated { string s = 1; }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("main.proto"),
            "syntax = \"proto3\";\nimport \"dep.proto\";\npackage m;\nmessage Main { d.Dep dep = 1; }\n",
        )
        .unwrap();

        compile_protos(
            &[root.join("main.proto")],
            &[root.join("include")],
            &root.join("out"),
            &BuildConfig::default(),
        )
        .unwrap();

        let code = std::fs::read_to_string(root.join("out/main.rs")).unwrap();
        assert!(code.contains("Dep"), "dependency type must be generated");
        assert!(code.contains("Main"));
        assert!(
            !code.contains("Unrelated"),
            "unrelated include-dir file must not leak into the output"
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
