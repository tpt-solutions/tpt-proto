//! Multi-file compilation pipeline (Phase 4 — §4.3, §6, §16).
//!
//! Builds on the single-file [`crate::compile`] step to provide:
//!
//! * import resolution with `public` re-export propagation and `weak`-import
//!   tolerance (including cycle detection and missing-import reporting),
//! * package/type-reference resolution across files (unqualified and qualified
//!   names resolved against the importing file's package and visible imports),
//! * cross-file duplicate symbol detection,
//! * option validation against a table of known file/message/field/enum options,
//! * editions feature resolution per file,
//! * a [`FileDescriptorSet`] output together with diagnostics and a
//!   [`LintReport`] hook for the lint crate.

use std::collections::{HashMap, HashSet};

use tpt_proto_descriptor::{
    DescriptorProto, FieldDescriptorProto, FieldType, FileDescriptorProto, FileDescriptorSet,
    ServiceDescriptorProto,
};
use tpt_proto_language::ast;
use tpt_proto_language::diagnostic::{Diagnostic, Diagnostics, ErrorCode, Severity, Span};

use crate::features::FeatureSet;

/// A single lint/diagnostic finding tied to a source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintFinding {
    /// Severity.
    pub severity: Severity,
    /// Stable error code.
    pub code: ErrorCode,
    /// Human-readable message.
    pub message: String,
    /// The file the finding relates to.
    pub file: String,
    /// Optional source span.
    pub span: Option<Span>,
}

/// A structured lint report produced alongside compilation.
///
/// This is the hook the `tpt-proto-lint` crate consumes for breaking-change and
/// style analysis; it aggregates every diagnostic with its originating file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LintReport {
    /// All findings across the compiled file set.
    pub findings: Vec<LintFinding>,
}

impl LintReport {
    /// Whether the report contains no findings.
    pub fn is_empty(&self) -> bool {
        self.findings.is_empty()
    }

    /// Whether the report contains any error-severity finding.
    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Error)
    }
}

/// The result of compiling a set of proto files together.
#[derive(Debug, Clone)]
pub struct CompileResult {
    /// The compiled descriptor set (all transitively visible files).
    pub set: FileDescriptorSet,
    /// All diagnostics emitted during compilation.
    pub diagnostics: Diagnostics,
    /// Structured lint findings (machine-readable hook for the lint crate).
    pub lint: LintReport,
    /// Resolved file-level feature set, keyed by file name.
    pub features: HashMap<String, FeatureSet>,
}

/// Resolve the descriptor `syntax` string and edition for an AST file.
fn file_syntax(f: &ast::File) -> (String, Option<String>) {
    if let Some(e) = &f.edition {
        ("editions".to_string(), Some(e.clone()))
    } else {
        let s = f
            .syntax
            .as_ref()
            .map(|s| s.value.clone())
            .unwrap_or_else(|| "proto2".to_string());
        (s, None)
    }
}

/// Compile a set of proto files together, resolving imports, packages, and
/// editions features.
///
/// All files in `files` are treated as the available compilation pool (roots
/// and their dependencies). A [`FileDescriptorSet`] containing every transitively
/// visible file is returned, along with diagnostics and a lint report.
pub fn compile_set(files: &[ast::File]) -> CompileResult {
    let mut diags = Diagnostics::new();
    let mut lint = LintReport::default();

    // Index files by name; duplicate file names are an error.
    let mut by_name: HashMap<String, &ast::File> = HashMap::new();
    for f in files {
        let n = f.name.clone();
        if by_name.insert(n.clone(), f).is_some() {
            let msg = format!("duplicate file name '{}'", n);
            diags.push(Diagnostic::error(
                ErrorCode::DuplicateSymbol,
                msg.clone(),
                None,
            ));
            lint.findings.push(LintFinding {
                severity: Severity::Error,
                code: ErrorCode::DuplicateSymbol,
                message: msg,
                file: n,
                span: None,
            });
        }
    }

    // Import cycle detection (must run before visibility fixpoint).
    detect_import_cycles(files, &by_name, &mut diags, &mut lint);

    // Visibility: which files' symbols each file can see (public re-export
    // closure; weak imports tolerated).
    let visible_files = compute_visibility(files, &by_name, &mut diags, &mut lint);

    // Convert each file (single-file step) and record resolved features.
    let mut converted: Vec<FileDescriptorProto> = Vec::new();
    let mut features: HashMap<String, FeatureSet> = HashMap::new();
    for f in files {
        let (fd, fdiags) = crate::compile(f);
        for d in fdiags.iter() {
            lint.findings.push(LintFinding {
                severity: d.severity,
                code: d.code,
                message: d.message.clone(),
                file: f.name.clone(),
                span: d.span,
            });
            diags.push(d.clone());
        }
        let (syn, ed) = file_syntax(f);
        features.insert(f.name.clone(), FeatureSet::for_syntax(&syn, ed.as_deref()));
        converted.push(fd);
    }

    // Build the symbol table across all files (also detects cross-file dups).
    let mut symbols: HashMap<String, String> = HashMap::new();
    let mut symbols_by_file: HashMap<String, HashSet<String>> = HashMap::new();
    for fd in &converted {
        let fname = fd.name.clone().unwrap_or_default();
        let set = symbols_by_file.entry(fname.clone()).or_default();
        collect_symbols(fd, &fname, &mut symbols, set, &mut diags, &mut lint);
    }

    // Resolve type references and validate options per file.
    for (f, fd) in files.iter().zip(converted.iter_mut()) {
        let mut visible_fqns: HashSet<String> =
            symbols_by_file.get(&f.name).cloned().unwrap_or_default();
        if let Some(vf) = visible_files.get(&f.name) {
            for dep in vf {
                if let Some(s) = symbols_by_file.get(dep) {
                    visible_fqns.extend(s.iter().cloned());
                }
            }
        }
        resolve_references(fd, &f.name, &visible_fqns, &mut diags, &mut lint);
        validate_options_in_file(f, &mut diags, &mut lint);
    }

    let set = FileDescriptorSet { file: converted };
    CompileResult {
        set,
        diagnostics: diags,
        lint,
        features,
    }
}

// ---------------------------------------------------------------------------
// Import graph: cycles and visibility.
// ---------------------------------------------------------------------------

fn detect_import_cycles(
    files: &[ast::File],
    by_name: &HashMap<String, &ast::File>,
    diags: &mut Diagnostics,
    lint: &mut LintReport,
) {
    // 0 = unvisited, 1 = in progress, 2 = done.
    let mut color: HashMap<String, u8> = HashMap::new();
    for f in files {
        visit_cycle(f.name.as_str(), by_name, &mut color, diags, lint);
    }
}

#[allow(clippy::only_used_in_recursion)]
fn visit_cycle(
    name: &str,
    by_name: &HashMap<String, &ast::File>,
    color: &mut HashMap<String, u8>,
    diags: &mut Diagnostics,
    lint: &mut LintReport,
) {
    match color.get(name).copied() {
        Some(2) => return,
        Some(1) => {
            let msg = format!("import cycle detected involving '{}'", name);
            diags.push(Diagnostic::error(ErrorCode::Other, msg.clone(), None));
            lint.findings.push(LintFinding {
                severity: Severity::Error,
                code: ErrorCode::Other,
                message: msg,
                file: name.to_string(),
                span: None,
            });
            return;
        }
        _ => {}
    }
    color.insert(name.to_string(), 1);
    if let Some(f) = by_name.get(name) {
        for imp in &f.imports {
            if by_name.contains_key(&imp.path) {
                visit_cycle(&imp.path, by_name, color, diags, lint);
            }
        }
    }
    color.insert(name.to_string(), 2);
}

fn compute_visibility(
    files: &[ast::File],
    by_name: &HashMap<String, &ast::File>,
    diags: &mut Diagnostics,
    lint: &mut LintReport,
) -> HashMap<String, HashSet<String>> {
    // Report missing non-weak imports.
    for f in files {
        for imp in &f.imports {
            if !by_name.contains_key(&imp.path) && imp.kind != ast::ImportKind::Weak {
                let msg = format!(
                    "cannot find imported file '{}' (imported by '{}')",
                    imp.path, f.name
                );
                diags.push(Diagnostic::error(
                    ErrorCode::Other,
                    msg.clone(),
                    Some(imp.span),
                ));
                lint.findings.push(LintFinding {
                    severity: Severity::Error,
                    code: ErrorCode::Other,
                    message: msg,
                    file: f.name.clone(),
                    span: Some(imp.span),
                });
            }
        }
    }

    // `reexported(name)`: the closure of `name`'s *public* imports — the set of
    // files whose symbols `name` re-exports to its own importers.
    let mut reexported: HashMap<String, Option<HashSet<String>>> = HashMap::new();
    let mut out: HashMap<String, HashSet<String>> = HashMap::new();
    for f in files {
        let mut vis = HashSet::new();
        for imp in &f.imports {
            if !by_name.contains_key(&imp.path) {
                // Missing weak import tolerated; missing non-weak reported above.
                continue;
            }
            // Any direct import (public or not) makes its symbols, and the
            // symbols it publicly re-exports, visible to this file.
            vis.insert(imp.path.clone());
            for s in reexported_of(&imp.path, by_name, &mut reexported) {
                vis.insert(s);
            }
        }
        out.insert(f.name.clone(), vis);
    }
    out
}

fn reexported_of(
    name: &str,
    by_name: &HashMap<String, &ast::File>,
    memo: &mut HashMap<String, Option<HashSet<String>>>,
) -> HashSet<String> {
    if let Some(cached) = memo.get(name) {
        return cached.clone().unwrap_or_default();
    }
    // Guard against cycles: a node currently being computed returns empty.
    memo.insert(name.to_string(), None);
    let mut set = HashSet::new();
    if let Some(f) = by_name.get(name) {
        for imp in &f.imports {
            if imp.kind == ast::ImportKind::Public && by_name.contains_key(&imp.path) {
                set.insert(imp.path.clone());
                for s in reexported_of(&imp.path, by_name, memo) {
                    set.insert(s);
                }
            }
        }
    }
    memo.insert(name.to_string(), Some(set.clone()));
    set
}

// ---------------------------------------------------------------------------
// Symbol table.
// ---------------------------------------------------------------------------

fn collect_symbols(
    fd: &FileDescriptorProto,
    file_name: &str,
    symbols: &mut HashMap<String, String>,
    by_file: &mut HashSet<String>,
    diags: &mut Diagnostics,
    lint: &mut LintReport,
) {
    let pkg = fd.package.as_deref().unwrap_or("");
    for m in &fd.message_type {
        collect_message(m, pkg, file_name, symbols, by_file, diags, lint);
    }
    for e in &fd.enum_type {
        let fqn = qualify(pkg, e.name.as_deref().unwrap_or(""));
        insert_symbol(symbols, by_file, &fqn, file_name, diags, lint);
    }
}

fn collect_message(
    m: &DescriptorProto,
    prefix: &str,
    file_name: &str,
    symbols: &mut HashMap<String, String>,
    by_file: &mut HashSet<String>,
    diags: &mut Diagnostics,
    lint: &mut LintReport,
) {
    let name = m.name.as_deref().unwrap_or("");
    let fqn = qualify(prefix, name);
    insert_symbol(symbols, by_file, &fqn, file_name, diags, lint);
    for n in &m.nested_type {
        collect_message(n, &fqn, file_name, symbols, by_file, diags, lint);
    }
    for e in &m.enum_type {
        let efqn = qualify(&fqn, e.name.as_deref().unwrap_or(""));
        insert_symbol(symbols, by_file, &efqn, file_name, diags, lint);
    }
}

fn insert_symbol(
    symbols: &mut HashMap<String, String>,
    by_file: &mut HashSet<String>,
    fqn: &str,
    file_name: &str,
    diags: &mut Diagnostics,
    lint: &mut LintReport,
) {
    by_file.insert(fqn.to_string());
    if let Some(prev) = symbols.get(fqn) {
        if prev != file_name {
            let msg = format!(
                "symbol '{}' defined in both '{}' and '{}'",
                fqn, prev, file_name
            );
            diags.push(Diagnostic::error(
                ErrorCode::DuplicateSymbol,
                msg.clone(),
                None,
            ));
            lint.findings.push(LintFinding {
                severity: Severity::Error,
                code: ErrorCode::DuplicateSymbol,
                message: msg,
                file: file_name.to_string(),
                span: None,
            });
        }
    } else {
        symbols.insert(fqn.to_string(), file_name.to_string());
    }
}

fn qualify(prefix: &str, name: &str) -> String {
    // Fully-qualified names always carry a leading dot. The prefix may itself
    // already be a leading-dot FQN (from a parent's recursion), so normalize it
    // first to avoid producing a double leading dot.
    let prefix = prefix.strip_prefix('.').unwrap_or(prefix);
    if prefix.is_empty() {
        format!(".{}", name)
    } else {
        format!(".{}.{}", prefix, name)
    }
}

// ---------------------------------------------------------------------------
// Type reference resolution.
// ---------------------------------------------------------------------------

fn resolve_references(
    fd: &mut FileDescriptorProto,
    file_name: &str,
    visible: &HashSet<String>,
    diags: &mut Diagnostics,
    lint: &mut LintReport,
) {
    let pkg = fd.package.clone().unwrap_or_default();
    for m in &mut fd.message_type {
        resolve_message(m, &pkg, file_name, visible, diags, lint);
    }
    for s in &mut fd.service {
        resolve_service(s, &pkg, visible, diags, lint);
    }
}

fn resolve_message(
    m: &mut DescriptorProto,
    prefix: &str,
    file_name: &str,
    visible: &HashSet<String>,
    diags: &mut Diagnostics,
    lint: &mut LintReport,
) {
    let name = m.name.clone().unwrap_or_default();
    let self_path = qualify(prefix, &name);
    for f in &mut m.field {
        resolve_field_type(f, prefix, visible, file_name, diags, lint);
    }
    for n in &mut m.nested_type {
        resolve_message(n, &self_path, file_name, visible, diags, lint);
    }
}

fn resolve_field_type(
    f: &mut FieldDescriptorProto,
    pkg: &str,
    visible: &HashSet<String>,
    file_name: &str,
    diags: &mut Diagnostics,
    lint: &mut LintReport,
) {
    let ty = f.r#type;
    if matches!(
        ty,
        Some(FieldType::Message) | Some(FieldType::Enum) | Some(FieldType::Group)
    ) {
        if let Some(tn) = f.type_name.clone() {
            match resolve_type_name(&tn, pkg, visible) {
                Some(resolved) => f.type_name = Some(resolved),
                None => {
                    let msg = format!(
                        "unknown type '{}' referenced by field '{}'",
                        tn,
                        f.name.clone().unwrap_or_default()
                    );
                    diags.push(Diagnostic::error(ErrorCode::UnknownType, msg.clone(), None));
                    lint.findings.push(LintFinding {
                        severity: Severity::Error,
                        code: ErrorCode::UnknownType,
                        message: msg,
                        file: file_name.to_string(),
                        span: None,
                    });
                }
            }
        }
    }
}

fn resolve_service(
    s: &mut ServiceDescriptorProto,
    pkg: &str,
    visible: &HashSet<String>,
    diags: &mut Diagnostics,
    lint: &mut LintReport,
) {
    for m in &mut s.method {
        if let Some(t) = m.input_type.clone() {
            if let Some(r) = resolve_type_name(&t, pkg, visible) {
                m.input_type = Some(r);
            } else {
                unknown_type(&t, &m.name.clone().unwrap_or_default(), diags, lint);
            }
        }
        if let Some(t) = m.output_type.clone() {
            if let Some(r) = resolve_type_name(&t, pkg, visible) {
                m.output_type = Some(r);
            } else {
                unknown_type(&t, &m.name.clone().unwrap_or_default(), diags, lint);
            }
        }
    }
}

fn unknown_type(name: &str, ctx: &str, diags: &mut Diagnostics, lint: &mut LintReport) {
    let msg = format!("unknown type '{}' referenced by method '{}'", name, ctx);
    diags.push(Diagnostic::error(ErrorCode::UnknownType, msg.clone(), None));
    lint.findings.push(LintFinding {
        severity: Severity::Error,
        code: ErrorCode::UnknownType,
        message: msg,
        file: String::new(),
        span: None,
    });
}

/// Resolve a type reference name to a fully-qualified name (with leading dot),
/// or `None` if it cannot be found in the visible symbol set.
fn resolve_type_name(name: &str, pkg: &str, visible: &HashSet<String>) -> Option<String> {
    if name.starts_with('.') {
        return if visible.contains(name) {
            Some(name.to_string())
        } else {
            None
        };
    }
    // `pkg` may be passed as a leading-dot FQN (nested scope); normalize it.
    let pkg = pkg.strip_prefix('.').unwrap_or(pkg);
    let parts: Vec<&str> = if pkg.is_empty() {
        Vec::new()
    } else {
        pkg.split('.').collect()
    };
    for i in (0..=parts.len()).rev() {
        let prefix = &parts[..i];
        let candidate = if prefix.is_empty() {
            format!(".{}", name)
        } else {
            format!(".{}.{}", prefix.join("."), name)
        };
        if visible.contains(&candidate) {
            return Some(candidate);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Option validation.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OptKind {
    Str,
    Bool,
    Enum(&'static [&'static str]),
}

fn value_matches(c: &ast::Constant, kind: OptKind) -> bool {
    match (c, kind) {
        (ast::Constant::String(_), OptKind::Str) => true,
        (ast::Constant::Bool(_), OptKind::Bool) => true,
        (ast::Constant::Ident(s), OptKind::Enum(variants)) => variants.contains(&s.as_str()),
        _ => false,
    }
}

fn validate_options(
    opts: &[ast::ProtoOption],
    known: &[(&str, OptKind)],
    ctx: &str,
    file: &str,
    diags: &mut Diagnostics,
    lint: &mut LintReport,
) {
    for o in opts {
        // Custom options `(ext.option)` are extension options; not validated here.
        if o.name.starts_with('(') {
            continue;
        }
        match known.iter().find(|(n, _)| *n == o.name) {
            Some((_, kind)) => {
                if !value_matches(&o.value, *kind) {
                    let msg = format!(
                        "option '{}' has a value of the wrong type ({})",
                        o.name, ctx
                    );
                    diags.push(Diagnostic::error(
                        ErrorCode::InvalidOption,
                        msg.clone(),
                        Some(o.span),
                    ));
                    lint.findings.push(LintFinding {
                        severity: Severity::Error,
                        code: ErrorCode::InvalidOption,
                        message: msg,
                        file: file.to_string(),
                        span: Some(o.span),
                    });
                }
            }
            None => {
                let msg = format!("unknown option '{}' ({})", o.name, ctx);
                diags.push(Diagnostic::warning(
                    ErrorCode::InvalidOption,
                    msg.clone(),
                    Some(o.span),
                ));
                lint.findings.push(LintFinding {
                    severity: Severity::Warning,
                    code: ErrorCode::InvalidOption,
                    message: msg,
                    file: file.to_string(),
                    span: Some(o.span),
                });
            }
        }
    }
}

const FILE_OPTIONS: &[(&str, OptKind)] = &[
    ("java_package", OptKind::Str),
    ("java_outer_classname", OptKind::Str),
    ("java_multiple_files", OptKind::Bool),
    ("go_package", OptKind::Str),
    ("cc_enable_arenas", OptKind::Bool),
    ("cc_generic_services", OptKind::Bool),
    ("java_generic_services", OptKind::Bool),
    ("py_generic_services", OptKind::Bool),
    (
        "optimize_for",
        OptKind::Enum(&["SPEED", "CODE_SIZE", "LITE_RUNTIME"]),
    ),
    ("deprecated", OptKind::Bool),
];

const MESSAGE_OPTIONS: &[(&str, OptKind)] = &[
    ("deprecated", OptKind::Bool),
    ("message_set_wire_format", OptKind::Bool),
];

const FIELD_OPTIONS: &[(&str, OptKind)] = &[
    ("deprecated", OptKind::Bool),
    ("packed", OptKind::Bool),
    ("ctype", OptKind::Enum(&["STRING", "CORD", "STRING_PIECE"])),
];

const ENUM_OPTIONS: &[(&str, OptKind)] = &[
    ("allow_alias", OptKind::Bool),
    ("deprecated", OptKind::Bool),
];

const ENUM_VALUE_OPTIONS: &[(&str, OptKind)] = &[("deprecated", OptKind::Bool)];

const METHOD_OPTIONS: &[(&str, OptKind)] = &[
    ("deprecated", OptKind::Bool),
    (
        "idempotency_level",
        OptKind::Enum(&["IDEMPOTENCY_UNKNOWN", "NO_SIDE_EFFECTS", "IDEMPOTENT"]),
    ),
];

fn validate_options_in_file(f: &ast::File, diags: &mut Diagnostics, lint: &mut LintReport) {
    let file = f.name.as_str();
    validate_options(&f.options, FILE_OPTIONS, "file option", file, diags, lint);
    for m in &f.messages {
        validate_message_options(m, file, diags, lint);
    }
    for e in &f.enums {
        validate_options(&e.options, ENUM_OPTIONS, "enum option", file, diags, lint);
        for v in &e.values {
            validate_options(
                &v.options,
                ENUM_VALUE_OPTIONS,
                "enum value option",
                file,
                diags,
                lint,
            );
        }
    }
    for s in &f.services {
        validate_options(&s.options, &[], "service option", file, diags, lint);
        for m in &s.methods {
            validate_options(
                &m.options,
                METHOD_OPTIONS,
                "method option",
                file,
                diags,
                lint,
            );
        }
    }
    for ext in &f.extensions {
        validate_options(&ext.options, &[], "extension option", file, diags, lint);
        for fld in &ext.fields {
            validate_options(
                &fld.options,
                FIELD_OPTIONS,
                "field option",
                file,
                diags,
                lint,
            );
        }
    }
}

fn validate_message_options(
    m: &ast::Message,
    file: &str,
    diags: &mut Diagnostics,
    lint: &mut LintReport,
) {
    validate_options(
        &m.options,
        MESSAGE_OPTIONS,
        "message option",
        file,
        diags,
        lint,
    );
    for f in &m.fields {
        validate_options(&f.options, FIELD_OPTIONS, "field option", file, diags, lint);
    }
    for o in &m.oneofs {
        validate_options(
            &o.options,
            &[("deprecated", OptKind::Bool)],
            "oneof option",
            file,
            diags,
            lint,
        );
        for f in &o.fields {
            validate_options(&f.options, FIELD_OPTIONS, "field option", file, diags, lint);
        }
    }
    for mf in &m.maps {
        validate_options(
            &mf.options,
            &[("deprecated", OptKind::Bool)],
            "map field option",
            file,
            diags,
            lint,
        );
    }
    for n in &m.nested_messages {
        validate_message_options(n, file, diags, lint);
    }
    for e in &m.nested_enums {
        validate_options(&e.options, ENUM_OPTIONS, "enum option", file, diags, lint);
        for v in &e.values {
            validate_options(
                &v.options,
                ENUM_VALUE_OPTIONS,
                "enum value option",
                file,
                diags,
                lint,
            );
        }
    }
    for ext in &m.nested_extends {
        validate_options(&ext.options, &[], "extension option", file, diags, lint);
        for f in &ext.fields {
            validate_options(&f.options, FIELD_OPTIONS, "field option", file, diags, lint);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::{EnumType, FieldPresence};
    use tpt_proto_language::parse_file;

    fn parse(name: &str, src: &str) -> ast::File {
        parse_file(name, src).file
    }

    #[test]
    fn resolves_imported_type() {
        let dep = parse(
            "dep.proto",
            r#"syntax = "proto3"; package dep; message Widget { int32 id = 1; }"#,
        );
        let root = parse(
            "root.proto",
            r#"
syntax = "proto3";
package root;
import "dep.proto";
message Container {
  dep.Widget w = 1;
  dep.Widget local = 2;
}
"#,
        );
        let res = compile_set(&[root, dep]);
        assert!(
            !res.diagnostics.has_errors(),
            "diags: {:?}",
            res.diagnostics.iter().collect::<Vec<_>>()
        );
        let c = res
            .set
            .find_file("root.proto")
            .unwrap()
            .find_message("Container")
            .unwrap();
        let w = c.find_field_by_name("w").unwrap();
        assert_eq!(w.type_name.as_deref(), Some(".dep.Widget"));
        let local = c.find_field_by_name("local").unwrap();
        assert_eq!(local.type_name.as_deref(), Some(".dep.Widget"));
    }

    #[test]
    fn public_import_reexports() {
        let a = parse(
            "a.proto",
            r#"syntax="proto3"; package a; message A { int32 x=1; }"#,
        );
        let b = parse(
            "b.proto",
            r#"syntax="proto3"; package b; import public "a.proto";"#,
        );
        let c = parse(
            "c.proto",
            r#"syntax="proto3"; package c; import "b.proto"; message C { a.A v = 1; }"#,
        );
        let res = compile_set(&[c, b, a]);
        assert!(
            !res.diagnostics.has_errors(),
            "diags: {:?}",
            res.diagnostics.iter().collect::<Vec<_>>()
        );
        let cf = res
            .set
            .find_file("c.proto")
            .unwrap()
            .find_message("C")
            .unwrap();
        assert_eq!(
            cf.find_field_by_name("v").unwrap().type_name.as_deref(),
            Some(".a.A")
        );
    }

    #[test]
    fn missing_import_is_error() {
        let root = parse(
            "r.proto",
            r#"syntax="proto3"; import "missing.proto"; message M { int32 x=1; }"#,
        );
        let res = compile_set(&[root]);
        assert!(res.diagnostics.has_errors());
    }

    #[test]
    fn weak_import_missing_is_ok() {
        let root = parse(
            "r.proto",
            r#"syntax="proto3"; import weak "missing.proto"; message M { int32 x=1; }"#,
        );
        let res = compile_set(&[root]);
        assert!(!res.diagnostics.has_errors());
    }

    #[test]
    fn unknown_type_is_error() {
        let root = parse(
            "r.proto",
            r#"syntax="proto3"; message M { Nonexistent x = 1; }"#,
        );
        let res = compile_set(&[root]);
        assert!(res.diagnostics.has_errors());
    }

    #[test]
    fn cross_file_duplicate_symbol_is_error() {
        let a = parse(
            "a.proto",
            r#"syntax="proto3"; package p; message Dup { int32 x=1; }"#,
        );
        let b = parse(
            "b.proto",
            r#"syntax="proto3"; package p; message Dup { int32 y=1; }"#,
        );
        let res = compile_set(&[a, b]);
        assert!(res.diagnostics.has_errors());
    }

    #[test]
    fn option_validation_unknown_and_wrong_type() {
        let root = parse(
            "r.proto",
            r#"syntax="proto3"; option go_package = "x"; option bogus_opt = 1; message M { int32 x = 1 [deprecated = "no"]; }"#,
        );
        let res = compile_set(&[root]);
        // bogus_opt -> warning; deprecated="no" is wrong type -> error.
        assert!(res.diagnostics.has_errors());
        let has_warn = res
            .lint
            .findings
            .iter()
            .any(|f| f.severity == Severity::Warning);
        assert!(has_warn);
    }

    #[test]
    fn editions_2024_features_resolved() {
        let root = parse("r.proto", r#"edition = "2024"; message M { int32 x = 1; }"#);
        let res = compile_set(&[root]);
        assert!(!res.diagnostics.has_errors());
        let f = res.features.get("r.proto").unwrap();
        assert_eq!(f.field_presence, FieldPresence::Explicit);
        assert_eq!(f.enum_type, EnumType::Closed);
    }

    #[test]
    fn import_cycle_detected() {
        let a = parse(
            "a.proto",
            r#"syntax="proto3"; import "b.proto"; message A {}"#,
        );
        let b = parse(
            "b.proto",
            r#"syntax="proto3"; import "a.proto"; message B {}"#,
        );
        let res = compile_set(&[a, b]);
        assert!(res.diagnostics.has_errors());
    }
}
