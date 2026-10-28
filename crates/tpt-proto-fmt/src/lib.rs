//! `tpt-proto-fmt` — formatter for `.proto` schema files.
//!
//! Formats proto2 / proto3 / editions sources deterministically while
//! **preserving every comment** (the language crate retains comments as
//! trivia alongside the token stream, so nothing is silently dropped).
//!
//! Design notes (v1):
//!
//! * Layout decisions are made over the merged stream of tokens and comments,
//!   using each item's original source line to preserve blank-line grouping
//!   and trailing comments. This keeps the formatter total (any file the
//!   lexer accepts can be formatted) and idempotent.
//! * Significant tokens are re-emitted from their **original source slices**
//!   (via spans), so literal spellings — hex numbers, string escapes,
//!   exponent forms — survive untouched.
//! * Attaching comments directly to AST nodes (rather than keeping them as a
//!   position-keyed side list) is tracked as follow-up work in `todo.md`
//!   (Phase 28); the public API here does not depend on that representation.
//!
//! Style (v1, intentionally minimal):
//!
//! * Two-space indentation; opening brace stays on the declaration line.
//! * One statement per line; `;` terminates the line.
//! * Single spaces around `=`; no spaces inside `map<k, v>` angle brackets,
//!   parentheses, brackets, or dotted names.
//! * At most one blank line between declarations (preserved from input).

use std::fmt;

use tpt_proto_language::{lex_with_comments, Comment, Token, TokenKind};

/// Errors from formatting (currently only lexical failures).
#[derive(Debug)]
pub enum FmtError {
    /// The source did not lex.
    Lex(tpt_proto_language::LexError),
}

impl fmt::Display for FmtError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FmtError::Lex(e) => write!(
                f,
                "{} at line {} column {}",
                e.message, e.pos.line, e.pos.column
            ),
        }
    }
}

impl std::error::Error for FmtError {}

impl From<tpt_proto_language::LexError> for FmtError {
    fn from(e: tpt_proto_language::LexError) -> Self {
        FmtError::Lex(e)
    }
}

/// Format `src`, returning the formatted output.
pub fn format_source(src: &str) -> Result<String, FmtError> {
    let (tokens, comments) = lex_with_comments(src)?;

    // Merge tokens and comments into one offset-ordered event stream.
    enum Ev<'a> {
        Tok(&'a Token),
        Cmt(&'a Comment),
    }
    impl Ev<'_> {
        fn start_offset(&self) -> usize {
            match self {
                Ev::Tok(t) => t.span.start.offset,
                Ev::Cmt(c) => c.span.start.offset,
            }
        }
        fn start_line(&self) -> u32 {
            match self {
                Ev::Tok(t) => t.span.start.line,
                Ev::Cmt(c) => c.span.start.line,
            }
        }
        fn end_line(&self) -> u32 {
            match self {
                Ev::Tok(t) => t.span.end.line,
                Ev::Cmt(c) => c.span.end.line,
            }
        }
        fn kind(&self) -> Option<&TokenKind> {
            match self {
                Ev::Tok(t) => Some(&t.kind),
                Ev::Cmt(_) => None,
            }
        }
    }

    let mut events: Vec<Ev<'_>> = Vec::with_capacity(tokens.len() + comments.len());
    events.extend(tokens.iter().map(Ev::Tok));
    events.extend(comments.iter().map(Ev::Cmt));
    events.sort_by_key(|ev| ev.start_offset());

    let mut out = String::with_capacity(src.len() + 64);
    let mut indent = 0usize;
    let mut at_bol = true;
    // Set right after an `{` whose body hasn't started yet, so `{}` stays
    // inline for empty bodies.
    let mut brace_open_inline = false;
    let mut prev: Option<&Ev<'_>> = None;

    macro_rules! newline {
        () => {{
            while out.ends_with([' ', '\t']) {
                out.pop();
            }
            out.push('\n');
            at_bol = true;
        }};
    }
    macro_rules! ensure_indent {
        () => {
            if at_bol {
                for _ in 0..indent {
                    out.push_str("  ");
                }
                at_bol = false;
            }
        };
    }

    for ev in &events {
        // Preserve at most one blank line where the source had a gap.
        let gap = prev
            .map(|p| ev.start_line() > p.end_line() + 1)
            .unwrap_or(false);

        match ev {
            Ev::Cmt(c) => {
                let trailing = prev
                    .map(|p| p.end_line() == ev.start_line())
                    .unwrap_or(false);
                if trailing && !at_bol {
                    out.push(' ');
                    out.push_str("//");
                    out.push_str(c.text.trim_start_matches('/').trim());
                } else {
                    if !at_bol || brace_open_inline {
                        if brace_open_inline {
                            indent += 1;
                            brace_open_inline = false;
                        }
                        newline!();
                    }
                    if gap {
                        out.push('\n');
                    }
                    ensure_indent!();
                    if c.text.trim_start().starts_with("/*") {
                        for line in c.text.lines() {
                            if at_bol {
                                ensure_indent!();
                            } else {
                                newline!();
                                ensure_indent!();
                            }
                            out.push_str(line.trim_end());
                        }
                        newline!();
                    } else {
                        out.push_str(c.text.trim());
                        newline!();
                    }
                }
                prev = Some(ev);
            }
            Ev::Tok(tok) => {
                let kind = &tok.kind;

                if matches!(kind, TokenKind::RBrace) && brace_open_inline {
                    // Empty body: close the brace on the same line (`{}`).
                    brace_open_inline = false;
                    ensure_indent!();
                    out.push('}');
                    newline!();
                    prev = Some(ev);
                    continue;
                }

                if matches!(kind, TokenKind::RBrace) {
                    if !at_bol {
                        newline!();
                    }
                    if gap {
                        out.push('\n');
                    }
                    indent = indent.saturating_sub(1);
                } else {
                    if brace_open_inline {
                        indent += 1;
                        brace_open_inline = false;
                        newline!();
                    }
                    if gap && at_bol {
                        out.push('\n');
                    } else if !at_bol {
                        let need_space = match (prev.and_then(|p| p.kind()), Some(kind)) {
                            (_, Some(TokenKind::Comma))
                            | (_, Some(TokenKind::Semicolon))
                            | (_, Some(TokenKind::RParen))
                            | (_, Some(TokenKind::RBracket))
                            | (_, Some(TokenKind::Gt))
                            | (_, Some(TokenKind::Dot)) => false,
                            (Some(TokenKind::Dot), _)
                            | (Some(TokenKind::LParen), _)
                            | (_, Some(TokenKind::LParen))
                            | (Some(TokenKind::LBracket), _)
                            | (_, Some(TokenKind::LBracket))
                            | (Some(TokenKind::Lt), _)
                            | (_, Some(TokenKind::Lt)) => false,
                            (Some(TokenKind::Equals), _) | (_, Some(TokenKind::Equals)) => true,
                            _ => true,
                        };
                        if need_space {
                            out.push(' ');
                        }
                    }
                }

                ensure_indent!();
                out.push_str(&src[tok.span.start.offset..tok.span.end.offset.min(src.len())]);

                match kind {
                    TokenKind::LBrace => {
                        brace_open_inline = true;
                    }
                    TokenKind::Semicolon => newline!(),
                    TokenKind::RBrace => newline!(),
                    _ => {}
                }
                prev = Some(ev);
            }
        }
    }

    if !out.ends_with('\n') {
        newline!();
    }
    Ok(out)
}

/// Check whether `src` is already formatted; returns `true` when unchanged.
///
/// Used by the CLI's `fmt --check` mode for CI.
pub fn check_source(src: &str) -> Result<bool, FmtError> {
    Ok(format_source(src)? == src)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_layout() {
        let src = "syntax=\"proto3\";package app;message M{int32 a=1;map<string,int32> m=2;}";
        let out = format_source(src).unwrap();
        assert_eq!(
            out,
            "syntax = \"proto3\";\npackage app;\nmessage M {\n  int32 a = 1;\n  map<string, int32> m = 2;\n}\n"
        );
    }

    #[test]
    fn preserves_comments_everywhere() {
        let src = "// header doc\nmessage M {\n  // leading\n  int32 a = 1; // trailing\n  /* block\n     comment */\n  string b = 2;\n}\n";
        let out = format_source(src).unwrap();
        assert!(out.contains("// header doc"));
        assert!(out.contains("// leading"));
        assert!(out.contains("// trailing"), "{out}");
        assert!(out.contains("/* block") && out.contains("comment */"));
    }

    #[test]
    fn idempotent() {
        let src = "syntax = \"proto3\";\n\n// doc\nmessage  M   {\n   string s=1;// t\n}\n";
        let once = format_source(src).unwrap();
        let twice = format_source(&once).unwrap();
        assert_eq!(once, twice, "formatter must be idempotent");
    }

    #[test]
    fn preserves_raw_literal_spellings() {
        // Hex/octal/exponent forms survive via source slices.
        let src = "option x = 0xFF; option y = 1.5e3; message M { int32 a = 010; }\n";
        let out = format_source(src).unwrap();
        assert!(out.contains("0xFF"), "{out}");
        assert!(out.contains("1.5e3"), "{out}");
        assert!(out.contains("010"), "{out}");
    }

    #[test]
    fn blank_line_grouping_capped_at_one() {
        let src = "message A {}\n\n\n\nmessage B {}\n";
        let out = format_source(src).unwrap();
        assert_eq!(out, "message A {}\n\nmessage B {}\n");
    }

    #[test]
    fn check_mode_detects_changes() {
        let ugly = "syntax=\"proto3\";";
        assert!(!check_source(ugly).unwrap());
        let nice = format_source(ugly).unwrap();
        assert!(check_source(&nice).unwrap());
    }

    #[test]
    fn lex_error_is_reported() {
        assert!(matches!(
            format_source("message { @ }"),
            Err(FmtError::Lex(_))
        ));
    }
}
