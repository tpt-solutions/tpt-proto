//! Regression tests: bracketed option/constant lists must terminate on
//! malformed input instead of spinning and growing the diagnostics vector
//! until the process runs out of memory (found by the `language_parser`
//! fuzz target).

use tpt_proto_language::{parse_file, Severity};

#[test]
fn unterminated_constant_list_with_stray_brace_terminates() {
    // `[[}` inside a statement: the inner `}` cannot start a constant.
    let src = "option\nL\n\n[[}a";
    let parsed = parse_file("bad.proto", src);
    assert!(parsed
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error));
}

#[test]
fn field_option_list_with_stray_brace_terminates() {
    let src = "syntax = \"proto3\";\nmessage M { int32 a = 1 [ } ; }";
    let parsed = parse_file("bad.proto", src);
    assert!(parsed
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error));
}
