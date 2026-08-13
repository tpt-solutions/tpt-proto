//! `tpt-proto-lint` — style and breaking-change detection (§4.13, §17).
//!
//! Compares an "old" and "new" schema and classifies the differences into
//! [`Severity::Safe`], [`Severity::Warning`], or [`Severity::Breaking`].

use tpt_proto_descriptor::{
    DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FileDescriptorProto,
    MethodDescriptorProto, ServiceDescriptorProto,
};

/// Classification of a schema change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// No compatibility impact.
    Safe,
    /// Potentially risky but not strictly breaking.
    Warning,
    /// Breaks wire/source compatibility for existing clients.
    Breaking,
}

impl Severity {
    /// Single-character / short tag for machine-readable output.
    pub fn tag(&self) -> &'static str {
        match self {
            Severity::Safe => "SAFE",
            Severity::Warning => "WARNING",
            Severity::Breaking => "BREAKING",
        }
    }
}

/// A single lint/breaking-change finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// Severity of the finding.
    pub severity: Severity,
    /// Stable finding code (e.g. `FIELD_REMOVED`).
    pub code: &'static str,
    /// Human-readable description.
    pub message: String,
}

/// A lint report: an ordered collection of findings.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// The findings.
    pub findings: Vec<Finding>,
}

impl Report {
    /// True if any finding is [`Severity::Breaking`].
    pub fn has_breaking(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Breaking)
    }

    /// Render the report in a simple human-readable form.
    pub fn render(&self) -> String {
        if self.findings.is_empty() {
            return "(no changes detected)\n".to_string();
        }
        let mut out = String::new();
        for f in &self.findings {
            out.push_str(&format!("[{}] {}: {}\n", f.severity.tag(), f.code, f.message));
        }
        out
    }

    /// Render the report as JSON.
    pub fn render_json(&self) -> String {
        let mut out = String::from("[\n");
        for (i, f) in self.findings.iter().enumerate() {
            let comma = if i + 1 < self.findings.len() { "," } else { "" };
            out.push_str(&format!(
                "  {{\"severity\":\"{}\",\"code\":\"{}\",\"message\":{}}}\n",
                f.severity.tag(),
                f.code,
                serde_json_escape(&f.message)
            ));
            out.push_str(comma);
            out.push('\n');
        }
        out.push_str("]\n");
        out
    }
}

fn serde_json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Lint the evolution from `old` to `new` (both top-level file descriptors).
pub fn lint(old: &FileDescriptorProto, new: &FileDescriptorProto) -> Report {
    let mut r = Report::default();
    lint_package(&mut r, old, new);
    lint_messages(&mut r, &old.message_type, &new.message_type, "");
    lint_enums(&mut r, &old.enum_type, &new.enum_type, "");
    lint_services(&mut r, &old.service, &new.service);
    r
}

fn lint_package(r: &mut Report, old: &FileDescriptorProto, new: &FileDescriptorProto) {
    let a = old.package.as_deref().unwrap_or("");
    let b = new.package.as_deref().unwrap_or("");
    if a != b {
        r.findings.push(Finding {
            severity: Severity::Breaking,
            code: "PACKAGE_CHANGED",
            message: format!("package changed from `{a}` to `{b}`"),
        });
    }
}

fn lint_messages(
    r: &mut Report,
    old: &[DescriptorProto],
    new: &[DescriptorProto],
    prefix: &str,
) {
    for nm in new {
        let name = format!("{prefix}{}", nm.name.clone().unwrap_or_default());
        let old_m = old.iter().find(|m| m.name == nm.name);
        match old_m {
            None => {
                r.findings.push(Finding {
                    severity: Severity::Safe,
                    code: "MESSAGE_ADDED",
                    message: format!("message `{name}` added"),
                });
            }
            Some(om) => {
                lint_message_fields(r, om, nm, &name);
                lint_messages(r, &om.nested_type, &nm.nested_type, &format!("{name}."));
                lint_enums(r, &om.enum_type, &nm.enum_type, &name);
            }
        }
    }
    for om in old {
        if !new.iter().any(|m| m.name == om.name) {
            let name = format!("{prefix}{}", om.name.clone().unwrap_or_default());
            r.findings.push(Finding {
                severity: Severity::Breaking,
                code: "MESSAGE_REMOVED",
                message: format!("message `{name}` removed"),
            });
        }
    }
}

fn lint_message_fields(r: &mut Report, old: &DescriptorProto, new: &DescriptorProto, msg: &str) {
    // Build maps keyed by number for reuse detection.
    let old_by_num: std::collections::HashMap<i32, &FieldDescriptorProto> =
        old.field.iter().map(|f| (f.number.unwrap_or(0), f)).collect();
    let mut new_names_by_num: std::collections::HashMap<i32, String> = std::collections::HashMap::new();

    for nf in &new.field {
        let num = nf.number.unwrap_or(0);
        let nname = nf.name.clone().unwrap_or_default();
        new_names_by_num.insert(num, nname.clone());
        match old_by_num.get(&num) {
            None => {
                r.findings.push(Finding {
                    severity: Severity::Warning,
                    code: "FIELD_ADDED",
                    message: format!("message `{msg}`: field `{nname}` (#{num}) added"),
                });
            }
            Some(of) => {
                let oname = of.name.clone().unwrap_or_default();
                if oname != nname {
                    r.findings.push(Finding {
                        severity: Severity::Breaking,
                        code: "FIELD_NUMBER_REUSED",
                        message: format!(
                            "message `{msg}`: field number #{num} reused (was `{oname}`, now `{nname}`)"
                        ),
                    });
                } else {
                    check_field_compat(r, msg, of, nf);
                }
            }
        }
    }

    for of in &old.field {
        let num = of.number.unwrap_or(0);
        if !new.field.iter().any(|f| f.number == Some(num)) {
            let reserved = new
                .reserved_range
                .iter()
                .any(|rr| rr.start <= num && (rr.end == 0 && rr.start == num || num <= rr.end));
            let reserved_name = new
                .reserved_name
                .iter()
                .any(|n| n == of.name.as_deref().unwrap_or(""));
            if reserved || reserved_name {
                r.findings.push(Finding {
                    severity: Severity::Safe,
                    code: "FIELD_RESERVED",
                    message: format!(
                        "message `{msg}`: field `{}` (#{num}) removed but declared reserved",
                        of.name.clone().unwrap_or_default()
                    ),
                });
            } else {
                r.findings.push(Finding {
                    severity: Severity::Breaking,
                    code: "FIELD_REMOVED",
                    message: format!(
                        "message `{msg}`: field `{}` (#{num}) removed without reservation",
                        of.name.clone().unwrap_or_default()
                    ),
                });
            }
        }
    }
}

fn check_field_compat(r: &mut Report, msg: &str, old: &FieldDescriptorProto, new: &FieldDescriptorProto) {
    let name = new.name.clone().unwrap_or_default();
    if old.r#type != new.r#type {
        r.findings.push(Finding {
            severity: Severity::Breaking,
            code: "FIELD_TYPE_CHANGED",
            message: format!(
                "message `{msg}`: field `{name}` type changed from {:?} to {:?}",
                old.r#type, new.r#type
            ),
        });
    }
    if old.label != new.label {
        r.findings.push(Finding {
            severity: Severity::Breaking,
            code: "FIELD_LABEL_CHANGED",
            message: format!(
                "message `{msg}`: field `{name}` label changed from {:?} to {:?}",
                old.label, new.label
            ),
        });
    }
    if old.type_name != new.type_name {
        r.findings.push(Finding {
            severity: Severity::Breaking,
            code: "FIELD_TYPE_NAME_CHANGED",
            message: format!(
                "message `{msg}`: field `{name}` type name changed from {:?} to {:?}",
                old.type_name, new.type_name
            ),
        });
    }
}

fn lint_enums(
    r: &mut Report,
    old: &[EnumDescriptorProto],
    new: &[EnumDescriptorProto],
    prefix: &str,
) {
    for ne in new {
        let name = format!("{prefix}{}", ne.name.clone().unwrap_or_default());
        match old.iter().find(|e| e.name == ne.name) {
            None => r.findings.push(Finding {
                severity: Severity::Safe,
                code: "ENUM_ADDED",
                message: format!("enum `{name}` added"),
            }),
            Some(oe) => {
                for ov in &oe.value {
                    if !ne.value.iter().any(|v| v.number == ov.number) {
                        r.findings.push(Finding {
                            severity: Severity::Breaking,
                            code: "ENUM_VALUE_REMOVED",
                            message: format!(
                                "enum `{name}`: value `{}` (#{}) removed",
                                ov.name.clone().unwrap_or_default(),
                                ov.number.unwrap_or(0)
                            ),
                        });
                    }
                }
                for nv in &ne.value {
                    if !oe.value.iter().any(|v| v.number == nv.number) {
                        r.findings.push(Finding {
                            severity: Severity::Warning,
                            code: "ENUM_VALUE_ADDED",
                            message: format!(
                                "enum `{name}`: value `{}` (#{}) added",
                                nv.name.clone().unwrap_or_default(),
                                nv.number.unwrap_or(0)
                            ),
                        });
                    }
                }
            }
        }
    }
    for oe in old {
        if !new.iter().any(|e| e.name == oe.name) {
            let name = format!("{prefix}{}", oe.name.clone().unwrap_or_default());
            r.findings.push(Finding {
                severity: Severity::Breaking,
                code: "ENUM_REMOVED",
                message: format!("enum `{name}` removed"),
            });
        }
    }
}

fn lint_services(
    r: &mut Report,
    old: &[ServiceDescriptorProto],
    new: &[ServiceDescriptorProto],
) {
    for ns in new {
        let name = ns.name.clone().unwrap_or_default();
        match old.iter().find(|s| s.name == ns.name) {
            None => r.findings.push(Finding {
                severity: Severity::Safe,
                code: "SERVICE_ADDED",
                message: format!("service `{name}` added"),
            }),
            Some(os) => lint_methods(r, os, ns),
        }
    }
    for os in old {
        if !new.iter().any(|s| s.name == os.name) {
            r.findings.push(Finding {
                severity: Severity::Breaking,
                code: "SERVICE_REMOVED",
                message: format!("service `{}` removed", os.name.clone().unwrap_or_default()),
            });
        }
    }
}

fn lint_methods(r: &mut Report, old: &ServiceDescriptorProto, new: &ServiceDescriptorProto) {
    for nm in &new.method {
        if !old.method.iter().any(|m| m.name == nm.name) {
            r.findings.push(Finding {
                severity: Severity::Warning,
                code: "METHOD_ADDED",
                message: format!("service `{}`: method `{}` added", new.name.clone().unwrap_or_default(), nm.name.clone().unwrap_or_default()),
            });
        } else if let Some(om) = old.method.iter().find(|m| m.name == nm.name) {
            check_method_compat(r, new.name.as_deref().unwrap_or(""), nm, om);
        }
    }
    for om in &old.method {
        if !new.method.iter().any(|m| m.name == om.name) {
            r.findings.push(Finding {
                severity: Severity::Breaking,
                code: "METHOD_REMOVED",
                message: format!("service `{}`: method `{}` removed", new.name.clone().unwrap_or_default(), om.name.clone().unwrap_or_default()),
            });
        }
    }
}

fn check_method_compat(
    r: &mut Report,
    service: &str,
    new: &MethodDescriptorProto,
    old: &MethodDescriptorProto,
) {
    let name = new.name.clone().unwrap_or_default();
    if old.input_type != new.input_type {
        r.findings.push(Finding {
            severity: Severity::Breaking,
            code: "METHOD_INPUT_CHANGED",
            message: format!("service `{service}`: method `{name}` input type changed"),
        });
    }
    if old.output_type != new.output_type {
        r.findings.push(Finding {
            severity: Severity::Breaking,
            code: "METHOD_OUTPUT_CHANGED",
            message: format!("service `{service}`: method `{name}` output type changed"),
        });
    }
    if old.client_streaming != new.client_streaming || old.server_streaming != new.server_streaming {
        r.findings.push(Finding {
            severity: Severity::Breaking,
            code: "METHOD_STREAMING_CHANGED",
            message: format!("service `{service}`: method `{name}` streaming mode changed"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_proto_descriptor::FieldType;

    fn msg(name: &str, fields: Vec<FieldDescriptorProto>) -> DescriptorProto {
        DescriptorProto { name: Some(name.into()), field: fields, ..Default::default() }
    }
    fn fld(name: &str, num: i32, ty: FieldType) -> FieldDescriptorProto {
        FieldDescriptorProto {
            name: Some(name.into()),
            number: Some(num),
            r#type: Some(ty),
            label: Some(tpt_proto_descriptor::Label::Optional),
            ..Default::default()
        }
    }

    #[test]
    fn detects_field_removal_without_reservation() {
        let old = FileDescriptorProto { message_type: vec![msg("M", vec![fld("a", 1, FieldType::String)])], ..Default::default() };
        let new = FileDescriptorProto { message_type: vec![msg("M", vec![])], ..Default::default() };
        let rep = lint(&old, &new);
        assert!(rep.has_breaking());
        assert!(rep.findings.iter().any(|f| f.code == "FIELD_REMOVED"));
    }

    #[test]
    fn reserved_field_is_safe() {
        let mut m = msg("M", vec![]);
        m.reserved_range.push(tpt_proto_descriptor::ReservedRange { start: 1, end: 0 });
        let old = FileDescriptorProto { message_type: vec![msg("M", vec![fld("a", 1, FieldType::String)])], ..Default::default() };
        let new = FileDescriptorProto { message_type: vec![m], ..Default::default() };
        let rep = lint(&old, &new);
        assert!(!rep.has_breaking());
        assert!(rep.findings.iter().any(|f| f.code == "FIELD_RESERVED"));
    }

    #[test]
    fn detects_type_change() {
        let old = FileDescriptorProto { message_type: vec![msg("M", vec![fld("a", 1, FieldType::String)])], ..Default::default() };
        let new = FileDescriptorProto { message_type: vec![msg("M", vec![fld("a", 1, FieldType::Int32)])], ..Default::default() };
        let rep = lint(&old, &new);
        assert!(rep.has_breaking());
        assert!(rep.findings.iter().any(|f| f.code == "FIELD_TYPE_CHANGED"));
    }

    #[test]
    fn added_field_is_warning() {
        let old = FileDescriptorProto { message_type: vec![msg("M", vec![])], ..Default::default() };
        let new = FileDescriptorProto { message_type: vec![msg("M", vec![fld("a", 1, FieldType::String)])], ..Default::default() };
        let rep = lint(&old, &new);
        assert!(!rep.has_breaking());
        assert!(rep.findings.iter().any(|f| f.code == "FIELD_ADDED"));
    }
}
