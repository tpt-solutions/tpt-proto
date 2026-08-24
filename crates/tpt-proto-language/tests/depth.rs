//! Regression tests for the `.proto` parser's recursion-depth guard
//! (`Parser::parse_message` / `MAX_PROTO_NESTING`): pathologically deep but
//! lexically valid sources must produce a diagnostic instead of overflowing
//! the stack, while legitimate nesting still parses cleanly.

use tpt_proto_language::{parse_file, Severity};

fn nested_source(levels: usize) -> String {
    let mut src = String::from("syntax = \"proto3\";\npackage deep;\n");
    for i in 0..levels {
        src.push_str(&format!("message M{i} {{\n"));
    }
    src.push_str("int32 leaf = 1;\n");
    for _ in 0..levels {
        src.push_str("}\n");
    }
    src
}

#[test]
fn pathological_nesting_is_rejected_not_stack_overflowed() {
    // 5_000 levels would overflow the recursive-descent parser unguarded.
    let src = nested_source(5_000);
    let parsed = parse_file("deep.proto", &src);
    let errors: Vec<_> = parsed
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        !errors.is_empty(),
        "pathologically deep input must produce an error diagnostic"
    );
    assert!(
        errors.iter().any(|d| d.message.contains("maximum depth")),
        "expected a depth-limit diagnostic, got: {:?}",
        errors.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn nesting_within_limit_parses_cleanly() {
    // Comfortably below MAX_PROTO_NESTING (200).
    let src = nested_source(150);
    let parsed = parse_file("ok.proto", &src);
    assert!(
        !parsed.diagnostics.has_errors(),
        "unexpected diagnostics: {:?}",
        parsed.diagnostics.iter().collect::<Vec<_>>()
    );
    // The outermost message exists with the innermost one nested beneath it.
    assert_eq!(parsed.file.messages.len(), 1);
    assert_eq!(parsed.file.messages[0].name.name, "M0");
}
