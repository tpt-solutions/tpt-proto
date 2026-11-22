//! `tpt-proto-text` — Protocol Buffers text format (§4.8, §13).
//!
//! Prints a descriptor-driven [`DynamicMessage`] to the protobuf text format
//! and parses text format back into a [`DynamicMessage`].

use std::sync::Arc;

use tpt_proto_core::{UnknownFieldSet, UnknownValue};
use tpt_proto_descriptor::{DescriptorProto, FieldDescriptorProto, FieldType, Label};
use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue as RScalar, Value};

/// Errors raised while parsing text format.
#[derive(Debug, thiserror::Error)]
pub enum TextError {
    /// A lexical error.
    #[error("lexical error at byte {0}: {1}")]
    Lex(u32, String),
    /// A syntax error.
    #[error("syntax error: {0}")]
    Syntax(String),
    /// An unknown field or type name.
    #[error("unknown `{0}`")]
    Unknown(String),
    /// A value that did not match the expected type.
    #[error("value error for `{0}`: {1}")]
    Value(String, String),
    /// Nesting depth exceeded the configured limit.
    #[error("recursion limit: {0}")]
    RecursionLimit(String),
}

/// Options controlling text output.
#[derive(Debug, Clone)]
pub struct TextOptions {
    /// Use `{ }` delimiters (default) vs `< >` delimiters.
    pub use_braces: bool,
    /// Emit field numbers instead of names.
    pub use_field_numbers: bool,
    /// Deterministic output: fields are emitted in ascending field-number order
    /// (rather than declaration order) and map entries are sorted by key.
    pub deterministic: bool,
    /// Maximum nesting depth accepted while parsing text format. Deeply nested
    /// input is rejected rather than recursing without bound (a stack-overflow
    /// DoS vector). Defaults to 100.
    pub max_depth: u32,
    /// Print fields the schema does not know, as `number: value` lines.
    pub print_unknown_fields: bool,
}

impl Default for TextOptions {
    fn default() -> Self {
        TextOptions {
            use_braces: false,
            use_field_numbers: false,
            deterministic: false,
            max_depth: 100,
            print_unknown_fields: false,
        }
    }
}

/// Print a [`DynamicMessage`] to text format.
pub fn message_to_text(
    pool: &DescriptorPool,
    descriptor: &DescriptorProto,
    msg: &DynamicMessage,
    opts: &TextOptions,
) -> String {
    // The top-level message has no enclosing delimiters: its fields are the
    // whole output, as in the canonical text format.
    let mut out = String::new();
    print_fields(pool, descriptor, msg, opts, 0, &mut out);
    out
}

/// A nested message: its fields between delimiters, one level deeper.
fn print_message(
    pool: &DescriptorPool,
    descriptor: &DescriptorProto,
    msg: &DynamicMessage,
    opts: &TextOptions,
    indent: usize,
    out: &mut String,
) {
    let open = if opts.use_braces { "{" } else { "<" };
    let close = if opts.use_braces { "}" } else { ">" };
    out.push_str(open);
    out.push('\n');
    print_fields(pool, descriptor, msg, opts, indent + 1, out);
    out.push_str(&"  ".repeat(indent));
    out.push_str(close);
    out.push('\n');
}

/// The fields of `msg`, each printed at `indent`.
fn print_fields(
    pool: &DescriptorPool,
    descriptor: &DescriptorProto,
    msg: &DynamicMessage,
    opts: &TextOptions,
    indent: usize,
    out: &mut String,
) {
    let mut fields: Vec<&FieldDescriptorProto> = descriptor
        .field
        .iter()
        .filter(|f| f.extendee.is_none())
        .collect();
    if opts.deterministic {
        fields.sort_by_key(|f| f.number.unwrap_or(0));
    }
    for field in fields {
        let num = field.number.unwrap_or(0);
        let name = if opts.use_field_numbers {
            num.to_string()
        } else if field.r#type == Some(FieldType::Group) {
            // A group is written under its type name (`Data {`).
            group_name(field).to_string()
        } else {
            field.name.clone().unwrap_or_default()
        };
        if let Some(value) = msg.get_field(num) {
            print_value(pool, field, &name, value, opts, indent, out);
        }
    }
    // Extension fields print as `[full.name]`.
    for (number, value) in &msg.extensions {
        let (Some(full_name), Some(ext)) = (
            pool.extension_full_name(*number),
            pool.get_extension(*number),
        ) else {
            continue;
        };
        let name = format!("[{full_name}]");
        print_value(pool, &ext, &name, value, opts, indent, out);
    }
    if opts.print_unknown_fields {
        print_unknown(&msg.unknown, indent, out);
    }
}

/// The short name of a group field's type (`Data` for `.pkg.Parent.Data`).
fn group_name(field: &FieldDescriptorProto) -> &str {
    field
        .type_name
        .as_deref()
        .and_then(|t| t.rsplit('.').next())
        .unwrap_or("")
}

/// The unknown fields in `bytes`, if the whole buffer parses as a message.
fn parse_unknown(bytes: &[u8]) -> Option<UnknownFieldSet> {
    let mut reader = tpt_proto_core::Reader::new(bytes);
    let mut set = UnknownFieldSet::new();
    while !reader.is_empty() {
        let tag = reader.read_tag().ok()?;
        set.store(tag, &mut reader).ok()?;
    }
    Some(set)
}

/// Unknown fields as `number: value` lines (groups as nested blocks).
fn print_unknown(set: &UnknownFieldSet, indent: usize, out: &mut String) {
    let pad = "  ".repeat(indent);
    for (number, value) in set.iter() {
        out.push_str(&pad);
        out.push_str(&number.to_string());
        match value {
            UnknownValue::Varint(v) => out.push_str(&format!(": {v}\n")),
            UnknownValue::Fixed32(v) => out.push_str(&format!(": 0x{v:08x}\n")),
            UnknownValue::Fixed64(v) => out.push_str(&format!(": 0x{v:016x}\n")),
            UnknownValue::LengthDelimited(b) => match parse_unknown(b) {
                // Bytes that parse as a message print as a nested block.
                Some(inner) => {
                    out.push_str(" {\n");
                    print_unknown(&inner, indent + 1, out);
                    out.push_str(&pad);
                    out.push_str("}\n");
                }
                None => {
                    out.push_str(": ");
                    push_c_escaped(b, out);
                    out.push('\n');
                }
            },
            UnknownValue::Group(inner) => {
                out.push_str(" {\n");
                print_unknown(inner, indent + 1, out);
                out.push_str(&pad);
                out.push_str("}\n");
            }
        }
    }
}

fn print_value(
    pool: &DescriptorPool,
    field: &FieldDescriptorProto,
    name: &str,
    value: &Value,
    opts: &TextOptions,
    indent: usize,
    out: &mut String,
) {
    let pad = "  ".repeat(indent);
    match value {
        Value::List(items) => {
            for it in items {
                print_value(pool, field, name, it, opts, indent, out);
            }
        }
        Value::Map(entries) => {
            let mut entries: Vec<&(Value, Value)> = entries.iter().collect();
            if opts.deterministic {
                entries.sort_by(|a, b| map_key_cmp(&a.0, &b.0));
            }
            for (k, v) in entries {
                out.push_str(&pad);
                out.push_str(name);
                out.push_str(" {\n");
                out.push_str(&"  ".repeat(indent + 1));
                out.push_str("key: ");
                print_scalar_inline(k, out);
                out.push('\n');
                out.push_str(&"  ".repeat(indent + 1));
                out.push_str("value: ");
                print_map_value(pool, field, v, opts, indent + 1, out);
                out.push('\n');
                out.push_str(&pad);
                out.push_str("}\n");
            }
        }
        other => {
            let t = field.r#type.unwrap_or(FieldType::String);
            let is_msg = matches!(t, FieldType::Message | FieldType::Group);
            out.push_str(&pad);
            out.push_str(name);
            if let Value::Message(_) = other {
                // nested message: short form `name { ... }`
                out.push(' ');
                print_message_inline(pool, field, other, opts, indent, out);
            } else if is_msg {
                out.push(' ');
                print_message_inline(pool, field, other, opts, indent, out);
            } else {
                out.push_str(": ");
                print_scalar_inline(other, out);
                out.push('\n');
            }
        }
    }
}

fn print_map_value(
    pool: &DescriptorPool,
    field: &FieldDescriptorProto,
    v: &Value,
    opts: &TextOptions,
    indent: usize,
    out: &mut String,
) {
    if let Value::Message(m) = v {
        if let Some(entry) = field
            .type_name
            .as_deref()
            .and_then(|t| pool.lookup_message(t))
        {
            if let Some(vf) = entry.field.iter().find(|f| f.number == Some(2)) {
                if let Some(sub) = vf.type_name.as_deref().and_then(|t| pool.lookup_message(t)) {
                    print_message(pool, &sub, m, opts, indent, out);
                    return;
                }
            }
        }
        print_message_inline(pool, field, v, opts, indent, out);
        return;
    }
    print_scalar_inline(v, out);
}

fn print_message_inline(
    pool: &DescriptorPool,
    field: &FieldDescriptorProto,
    value: &Value,
    opts: &TextOptions,
    indent: usize,
    out: &mut String,
) {
    if let Value::Message(dm) = value {
        let sub = pool
            .lookup_message(field.type_name.as_deref().unwrap_or(""))
            .unwrap_or_else(|| Arc::new(DescriptorProto::default()));
        print_message(pool, &sub, dm, opts, indent, out);
    }
}

/// Total order over map keys used for deterministic text output. Keys compare
/// first by a stable type rank, then by value.
fn map_key_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    use RScalar::*;
    fn rank(v: &Value) -> u8 {
        match v {
            Value::Scalar(String(_)) => 0,
            Value::Scalar(Bytes(_)) => 1,
            Value::Scalar(Bool(_)) => 2,
            Value::Scalar(I64(_)) => 3,
            Value::Scalar(U64(_)) => 4,
            Value::Scalar(F64(_)) => 5,
            _ => 6,
        }
    }
    let ra = rank(a);
    let rb = rank(b);
    if ra != rb {
        return ra.cmp(&rb);
    }
    match (a, b) {
        (Value::Scalar(String(x)), Value::Scalar(String(y))) => x.cmp(y),
        (Value::Scalar(Bytes(x)), Value::Scalar(Bytes(y))) => x.cmp(y),
        (Value::Scalar(Bool(x)), Value::Scalar(Bool(y))) => x.cmp(y),
        (Value::Scalar(I64(x)), Value::Scalar(I64(y))) => x.cmp(y),
        (Value::Scalar(U64(x)), Value::Scalar(U64(y))) => x.cmp(y),
        (Value::Scalar(F64(x)), Value::Scalar(F64(y))) => {
            x.partial_cmp(y).unwrap_or(Ordering::Equal)
        }
        _ => Ordering::Equal,
    }
}

/// A quoted C-escaped literal, as the text format writes strings and bytes:
/// printable ASCII as-is, the common escapes by name, anything else as octal.
fn push_c_escaped(bytes: &[u8], out: &mut String) {
    out.push('"');
    for &b in bytes {
        match b {
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            b'"' => out.push_str("\\\""),
            b'\'' => out.push_str("\\'"),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7e => out.push(b as char),
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out.push('"');
}

fn print_scalar_inline(v: &Value, out: &mut String) {
    match v {
        Value::Scalar(RScalar::String(s)) => push_c_escaped(s.as_bytes(), out),
        Value::Scalar(RScalar::Bytes(b)) => push_c_escaped(b, out),
        Value::Scalar(RScalar::Bool(b)) => out.push_str(if *b { "true" } else { "false" }),
        Value::Scalar(RScalar::F64(x)) => {
            if x.is_nan() {
                out.push_str("nan");
            } else if x.is_infinite() {
                out.push_str(if *x > 0.0 { "inf" } else { "-inf" });
            } else {
                out.push_str(&x.to_string());
            }
        }
        Value::Scalar(RScalar::I64(x)) => out.push_str(&x.to_string()),
        Value::Scalar(RScalar::U64(x)) => out.push_str(&x.to_string()),
        Value::Enum(n) => out.push_str(&n.to_string()),
        _ => out.push('?'),
    }
}

// ---------------------------------------------------------------------------
// Parsing.
// ---------------------------------------------------------------------------

/// Parse a text-format string into a [`DynamicMessage`].
pub fn text_to_message(
    pool: &DescriptorPool,
    descriptor: &DescriptorProto,
    input: &str,
    opts: &TextOptions,
) -> Result<DynamicMessage, TextError> {
    let tokens = tokenize(input)?;
    let mut p = Parser {
        toks: tokens,
        pos: 0,
        opts,
    };
    let mut msg = DynamicMessage::new(Arc::new(descriptor.clone()), pool.clone());
    // Skip the optional top-level message delimiter (`{`/`}` or `<`/`>`).
    if matches!(p.peek(), Some(Tok::LBrace) | Some(Tok::LAngle)) {
        p.next();
    }
    p.parse_message_body(pool, descriptor, &mut msg, 0)?;
    if matches!(p.peek(), Some(Tok::RBrace) | Some(Tok::RAngle)) {
        p.next();
    }
    Ok(msg)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    /// An identifier or keyword (`optional_int32`, `true`, `inf`).
    Ident(String),
    /// A string literal after escape processing. Adjacent literals concatenate.
    Str(Vec<u8>),
    /// A numeric literal as written; a sign is a separate `Minus` token.
    Num(String),
    Minus,
    Slash,
    Colon,
    Comma,
    Semi,
    LBrace,
    RBrace,
    LAngle,
    RAngle,
    LBracket,
    RBracket,
}

fn lex_error(pos: usize, message: &str) -> TextError {
    TextError::Lex(pos as u32, message.to_string())
}

fn tokenize(input: &str) -> Result<Vec<Tok>, TextError> {
    let bytes = input.as_bytes();
    let n = bytes.len();
    let mut i = 0;
    let mut toks = Vec::new();
    while i < n {
        let c = bytes[i];
        let single = match c {
            b' ' | b'\t' | b'\r' | b'\n' => {
                i += 1;
                continue;
            }
            b'#' => {
                while i < n && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'"' | b'\'' => {
                let (s, next) = parse_string(bytes, i)?;
                toks.push(Tok::Str(s));
                i = next;
                continue;
            }
            b'{' => Tok::LBrace,
            b'}' => Tok::RBrace,
            b'<' => Tok::LAngle,
            b'>' => Tok::RAngle,
            b'[' => Tok::LBracket,
            b']' => Tok::RBracket,
            b':' => Tok::Colon,
            b',' => Tok::Comma,
            b';' => Tok::Semi,
            b'-' => Tok::Minus,
            b'/' => Tok::Slash,
            _ if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < n && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'.'))
                {
                    i += 1;
                }
                toks.push(Tok::Ident(input[start..i].to_string()));
                continue;
            }
            _ if c.is_ascii_digit() || c == b'.' => {
                let start = i;
                let hex = input[start..].starts_with("0x") || input[start..].starts_with("0X");
                i += 1;
                while i < n {
                    let d = bytes[i];
                    // An exponent sign belongs to the number (but not in hex).
                    let exp_sign =
                        matches!(d, b'+' | b'-') && matches!(bytes[i - 1], b'e' | b'E') && !hex;
                    if d.is_ascii_alphanumeric() || d == b'.' || exp_sign {
                        i += 1;
                    } else {
                        break;
                    }
                }
                toks.push(Tok::Num(input[start..i].to_string()));
                continue;
            }
            _ => return Err(lex_error(i, &format!("unexpected byte {:?}", c as char))),
        };
        toks.push(single);
        i += 1;
    }
    Ok(toks)
}

fn hex_digit(b: u8) -> Option<u32> {
    (b as char).to_digit(16)
}

/// Read exactly `width` hex digits starting at `at`.
fn read_hex(bytes: &[u8], at: usize, width: usize) -> Option<u32> {
    let digits = bytes.get(at..at.checked_add(width)?)?;
    digits
        .iter()
        .try_fold(0u32, |acc, &b| Some(acc * 16 + hex_digit(b)?))
}

/// Parse a quoted string starting at `start`; returns its bytes after escape
/// processing and the index just past the closing quote.
fn parse_string(bytes: &[u8], start: usize) -> Result<(Vec<u8>, usize), TextError> {
    let quote = bytes[start];
    let n = bytes.len();
    let mut i = start + 1;
    let mut out = Vec::new();
    while i < n {
        let c = bytes[i];
        if c == quote {
            return Ok((out, i + 1));
        }
        if c == b'\n' || c == b'\r' {
            return Err(lex_error(i, "newline in string literal"));
        }
        if c != b'\\' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        let Some(&e) = bytes.get(i) else { break };
        i += 1;
        match e {
            b'a' => out.push(7),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(11),
            b'\\' | b'\'' | b'"' | b'?' => out.push(e),
            b'0'..=b'7' => {
                let mut v = u32::from(e - b'0');
                let mut digits = 1;
                while digits < 3 && i < n && (b'0'..=b'7').contains(&bytes[i]) {
                    v = v * 8 + u32::from(bytes[i] - b'0');
                    i += 1;
                    digits += 1;
                }
                if v > 0xFF {
                    return Err(lex_error(i, "octal escape out of range"));
                }
                out.push(v as u8);
            }
            b'x' | b'X' => {
                let mut v = 0u32;
                let mut digits = 0;
                while digits < 2 && i < n {
                    let Some(d) = hex_digit(bytes[i]) else { break };
                    v = v * 16 + d;
                    i += 1;
                    digits += 1;
                }
                if digits == 0 {
                    return Err(lex_error(i, "\\x needs hex digits"));
                }
                out.push(v as u8);
            }
            b'u' | b'U' => {
                let width = if e == b'u' { 4 } else { 8 };
                let cp =
                    read_hex(bytes, i, width).ok_or_else(|| lex_error(i, "bad unicode escape"))?;
                i += width;
                // Surrogate code points are not accepted, not even as pairs.
                let ch = char::from_u32(cp)
                    .ok_or_else(|| lex_error(i, "unicode escape is not a character"))?;
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
            }
            _ => return Err(lex_error(i, "unknown escape sequence")),
        }
    }
    Err(lex_error(start, "unterminated string"))
}

/// The magnitude of an integer literal: decimal, `0x` hex, or leading-`0` octal.
fn int_literal(text: &str) -> Option<u128> {
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u128::from_str_radix(hex, 16).ok()
    } else if text.len() > 1 && text.starts_with('0') {
        u128::from_str_radix(&text[1..], 8).ok()
    } else {
        text.parse::<u128>().ok()
    }
}

/// Apply a sign to an integer literal's magnitude.
fn signed_value(magnitude: u128, negative: bool) -> Option<i128> {
    let v = i128::try_from(magnitude).ok()?;
    Some(if negative { -v } else { v })
}

/// Parse a decimal floating-point literal (with an optional `f` suffix).
fn float_literal(text: &str, negative: bool) -> Result<f64, String> {
    if text.starts_with("0x") || text.starts_with("0X") {
        return Err("hexadecimal is not a float".into());
    }
    let body = text
        .strip_suffix(['f', 'F'])
        .filter(|b| !b.is_empty())
        .unwrap_or(text);
    let v: f64 = body
        .parse()
        .map_err(|_| format!("invalid float {text:?}"))?;
    Ok(if negative { -v } else { v })
}

/// Convert an integer value to the scalar representation of an integer field,
/// rejecting values outside the field's range.
fn int_in_range(t: FieldType, v: i128) -> Option<RScalar> {
    use RScalar::*;
    let fits = |lo: i128, hi: i128| (lo..=hi).contains(&v);
    match t {
        FieldType::Int32 | FieldType::Sint32 | FieldType::Sfixed32 => {
            fits(i128::from(i32::MIN), i128::from(i32::MAX)).then_some(I64(v as i64))
        }
        FieldType::Int64 | FieldType::Sint64 | FieldType::Sfixed64 => {
            fits(i128::from(i64::MIN), i128::from(i64::MAX)).then_some(I64(v as i64))
        }
        FieldType::Uint32 | FieldType::Fixed32 => {
            fits(0, i128::from(u32::MAX)).then_some(U64(v as u64))
        }
        FieldType::Uint64 | FieldType::Fixed64 => {
            fits(0, i128::from(u64::MAX)).then_some(U64(v as u64))
        }
        _ => None,
    }
}

struct Parser<'a> {
    toks: Vec<Tok>,
    pos: usize,
    opts: &'a TextOptions,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, t: &Tok) -> Result<(), TextError> {
        if self.eat(t) {
            Ok(())
        } else {
            Err(TextError::Syntax(format!(
                "expected {:?}, got {:?}",
                t,
                self.peek()
            )))
        }
    }

    fn parse_message_body(
        &mut self,
        pool: &DescriptorPool,
        descriptor: &DescriptorProto,
        msg: &mut DynamicMessage,
        depth: u32,
    ) -> Result<(), TextError> {
        if depth > self.opts.max_depth {
            return Err(TextError::RecursionLimit(format!(
                "text nesting exceeds max_depth {}",
                self.opts.max_depth
            )));
        }
        loop {
            match self.peek().cloned() {
                None | Some(Tok::RBrace) | Some(Tok::RAngle) => return Ok(()),
                Some(Tok::LBracket) => {
                    self.parse_bracketed(pool, descriptor, msg, depth)?;
                }
                Some(Tok::Ident(name)) => {
                    self.pos += 1;
                    self.parse_field(pool, descriptor, msg, &name, depth)?;
                }
                Some(other) => {
                    return Err(TextError::Syntax(format!(
                        "unexpected token in message body: {other:?}"
                    )));
                }
            }
        }
    }

    /// `{ ... }` or `< ... >` as a nested message body.
    fn parse_braced_body(
        &mut self,
        pool: &DescriptorPool,
        descriptor: &DescriptorProto,
        msg: &mut DynamicMessage,
        depth: u32,
    ) -> Result<(), TextError> {
        match self.next() {
            Some(Tok::LBrace) | Some(Tok::LAngle) => {}
            other => {
                return Err(TextError::Syntax(format!(
                    "expected message body, got {other:?}"
                )))
            }
        }
        self.parse_message_body(pool, descriptor, msg, depth + 1)?;
        self.close_delim()
    }

    fn close_delim(&mut self) -> Result<(), TextError> {
        match self.next() {
            Some(Tok::RBrace) | Some(Tok::RAngle) => Ok(()),
            other => Err(TextError::Syntax(format!(
                "expected closing delimiter, got {other:?}"
            ))),
        }
    }

    /// `[name] { ... }`: an `Any` entry (`[type.googleapis.com/Type]`) inside an
    /// `Any` body, or an extension field `[full.name]: value`.
    fn parse_bracketed(
        &mut self,
        pool: &DescriptorPool,
        descriptor: &DescriptorProto,
        msg: &mut DynamicMessage,
        depth: u32,
    ) -> Result<(), TextError> {
        self.expect(&Tok::LBracket)?;
        let mut name = String::new();
        loop {
            match self.next() {
                Some(Tok::RBracket) => break,
                Some(Tok::Ident(s)) => name.push_str(&s),
                Some(Tok::Slash) => name.push('/'),
                other => {
                    return Err(TextError::Syntax(format!(
                        "bad name in brackets: {other:?}"
                    )))
                }
            }
        }
        if descriptor.name.as_deref() == Some("Any") {
            let type_name = name.rsplit('/').next().unwrap_or(&name).to_string();
            let target = pool
                .lookup_message(&type_name)
                .ok_or_else(|| TextError::Unknown(type_name.clone()))?;
            let mut inner = DynamicMessage::new(target.clone(), pool.clone());
            self.parse_braced_body(pool, &target, &mut inner, depth)?;
            let bytes = inner
                .encode()
                .map_err(|e| TextError::Value(name.clone(), e.to_string()))?;
            msg.set_field(1, Value::Scalar(RScalar::String(name.clone())));
            msg.set_field(2, Value::Scalar(RScalar::Bytes(bytes)));
        } else {
            let ext = pool
                .get_extension_by_name(&name)
                .ok_or_else(|| TextError::Unknown(name.clone()))?;
            let number = ext.number.unwrap_or(0);
            let value = self.parse_field_value(pool, &ext, depth)?;
            msg.set_extension(number, value);
        }
        self.eat_separator();
        Ok(())
    }

    fn eat_separator(&mut self) {
        if !self.eat(&Tok::Comma) {
            self.eat(&Tok::Semi);
        }
    }

    fn parse_field(
        &mut self,
        pool: &DescriptorPool,
        descriptor: &DescriptorProto,
        msg: &mut DynamicMessage,
        name: &str,
        depth: u32,
    ) -> Result<(), TextError> {
        // Look up the field by name or number.
        let field = descriptor
            .field
            .iter()
            .find(|f| {
                f.name.as_deref() == Some(name)
                    || f.number == name.parse::<i32>().ok()
                    || (f.r#type == Some(FieldType::Group) && group_name(f) == name)
            })
            .cloned()
            .ok_or_else(|| TextError::Unknown(name.to_string()))?;
        let num = field.number.unwrap_or(0);
        let is_map = is_map_field(pool, &field);

        if field.label == Some(Label::Repeated)
            && !is_map
            && self.peek() == Some(&Tok::Colon)
            && self.toks.get(self.pos + 1) == Some(&Tok::LBracket)
        {
            // `name: [v1, v2]`: a list of values.
            self.pos += 2;
            if !self.eat(&Tok::RBracket) {
                loop {
                    let value = self.parse_element(pool, &field, depth)?;
                    push_repeated(msg, num, value);
                    if !self.eat(&Tok::Comma) {
                        self.expect(&Tok::RBracket)?;
                        break;
                    }
                }
            }
        } else {
            let value = self.parse_field_value(pool, &field, depth)?;
            if is_map {
                if let Value::Map(mut entries) = map_entry_to_value(value) {
                    if let Some((k, v)) = entries.pop() {
                        push_repeated(msg, num, Value::Map(vec![(k, v)]));
                    }
                }
            } else if field.label == Some(Label::Repeated) {
                push_repeated(msg, num, value);
            } else {
                msg.set_field(num, value);
            }
        }
        self.eat_separator();
        Ok(())
    }

    /// The value after a field name: `: value`, or a message body for message fields.
    fn parse_field_value(
        &mut self,
        pool: &DescriptorPool,
        field: &FieldDescriptorProto,
        depth: u32,
    ) -> Result<Value, TextError> {
        let is_message = matches!(
            field.r#type,
            Some(FieldType::Message) | Some(FieldType::Group)
        );
        let colon = self.eat(&Tok::Colon);
        if is_message && matches!(self.peek(), Some(Tok::LBrace) | Some(Tok::LAngle)) {
            return self.parse_message_value(pool, field, depth);
        }
        if !colon {
            return Err(TextError::Syntax(format!(
                "expected ':' after field {}",
                field.name.clone().unwrap_or_default()
            )));
        }
        self.parse_value(pool, field, depth)
    }

    /// One element of a repeated field (a message body or a scalar/enum).
    fn parse_element(
        &mut self,
        pool: &DescriptorPool,
        field: &FieldDescriptorProto,
        depth: u32,
    ) -> Result<Value, TextError> {
        let is_message = matches!(
            field.r#type,
            Some(FieldType::Message) | Some(FieldType::Group)
        );
        if is_message && matches!(self.peek(), Some(Tok::LBrace) | Some(Tok::LAngle)) {
            return self.parse_message_value(pool, field, depth);
        }
        self.parse_value(pool, field, depth)
    }

    fn parse_message_value(
        &mut self,
        pool: &DescriptorPool,
        field: &FieldDescriptorProto,
        depth: u32,
    ) -> Result<Value, TextError> {
        let sub = pool
            .lookup_message(field.type_name.as_deref().unwrap_or(""))
            .ok_or_else(|| TextError::Unknown(field.type_name.clone().unwrap_or_default()))?;
        let mut inner = DynamicMessage::new(sub.clone(), pool.clone());
        self.parse_braced_body(pool, &sub, &mut inner, depth)?;
        Ok(Value::Message(inner))
    }

    /// A single scalar or enum value.
    fn parse_value(
        &mut self,
        pool: &DescriptorPool,
        field: &FieldDescriptorProto,
        depth: u32,
    ) -> Result<Value, TextError> {
        let _ = depth;
        let fname = field.name.clone().unwrap_or_default();
        let err = |msg: &str| TextError::Value(fname.clone(), msg.to_string());
        let t = field.r#type.unwrap_or(FieldType::String);
        let negative = self.eat(&Tok::Minus);
        let tok = self
            .next()
            .ok_or_else(|| TextError::Syntax("expected value".into()))?;

        if t == FieldType::Enum {
            let number = match tok {
                Tok::Ident(name) if !negative => {
                    let edesc = pool
                        .lookup_enum(field.type_name.as_deref().unwrap_or(""))
                        .ok_or_else(|| {
                            TextError::Unknown(field.type_name.clone().unwrap_or_default())
                        })?;
                    edesc
                        .value
                        .iter()
                        .find(|v| v.name.as_deref() == Some(name.as_str()))
                        .and_then(|v| v.number)
                        .ok_or_else(|| err(&format!("unknown enum value {name}")))?
                }
                Tok::Num(text) => {
                    let v = int_literal(&text)
                        .and_then(|m| signed_value(m, negative))
                        .and_then(|v| i32::try_from(v).ok())
                        .ok_or_else(|| err("bad enum number"))?;
                    // Closed enums (proto2) reject numbers they do not define.
                    if pool.syntax == "proto2" {
                        let edesc = pool
                            .lookup_enum(field.type_name.as_deref().unwrap_or(""))
                            .ok_or_else(|| {
                                TextError::Unknown(field.type_name.clone().unwrap_or_default())
                            })?;
                        if !edesc.value.iter().any(|x| x.number == Some(v)) {
                            return Err(err("unknown closed enum number"));
                        }
                    }
                    v
                }
                _ => return Err(err("expected enum value")),
            };
            return Ok(Value::Enum(number));
        }

        let scalar = match (t, tok) {
            (FieldType::String | FieldType::Bytes, Tok::Str(first)) if !negative => {
                // Adjacent string literals concatenate.
                let mut bytes = first;
                while let Some(Tok::Str(more)) = self.peek().cloned() {
                    self.pos += 1;
                    bytes.extend(more);
                }
                if t == FieldType::String {
                    RScalar::String(
                        String::from_utf8(bytes)
                            .map_err(|_| err("string field is not valid UTF-8"))?,
                    )
                } else {
                    RScalar::Bytes(bytes)
                }
            }
            (FieldType::Bool, Tok::Ident(word)) if !negative => match word.as_str() {
                "true" | "True" | "t" => RScalar::Bool(true),
                "false" | "False" | "f" => RScalar::Bool(false),
                _ => return Err(err(&format!("bad bool {word}"))),
            },
            (FieldType::Bool, Tok::Num(text)) => match int_literal(&text) {
                Some(0) if !negative => RScalar::Bool(false),
                Some(1) if !negative => RScalar::Bool(true),
                _ => return Err(err("bool must be 0 or 1")),
            },
            (FieldType::Float | FieldType::Double, Tok::Ident(word)) => {
                let lower = word.to_ascii_lowercase();
                let v = match lower.as_str() {
                    "inf" | "infinity" => f64::INFINITY,
                    "nan" if !negative => f64::NAN,
                    _ => return Err(err(&format!("bad float literal {word}"))),
                };
                RScalar::F64(if negative { -v } else { v })
            }
            (FieldType::Float | FieldType::Double, Tok::Num(text)) => {
                // A leading zero would make an octal integer; floats do not accept one.
                if text.len() > 1
                    && text.starts_with('0')
                    && text.bytes().all(|b| b.is_ascii_digit())
                {
                    return Err(err("leading zero is not allowed in a float"));
                }
                // Out-of-range magnitudes overflow to infinity, as the spec allows.
                RScalar::F64(float_literal(&text, negative).map_err(|e| err(&e))?)
            }
            (_, Tok::Num(text)) => {
                let v = int_literal(&text)
                    .and_then(|m| signed_value(m, negative))
                    .ok_or_else(|| err(&format!("bad integer {text}")))?;
                int_in_range(t, v).ok_or_else(|| err("integer out of range"))?
            }
            _ => return Err(err("unexpected token")),
        };
        Ok(Value::Scalar(scalar))
    }
}

/// Append one element to a repeated (or map) field.
fn push_repeated(msg: &mut DynamicMessage, number: i32, value: Value) {
    // Map entries accumulate in a map; other elements in a list.
    let is_map = matches!(value, Value::Map(_));
    let entry = msg.fields.entry(number).or_insert_with(|| {
        if is_map {
            Value::Map(Vec::new())
        } else {
            Value::List(Vec::new())
        }
    });
    match (entry, value) {
        (Value::List(list), Value::List(mut more)) => list.append(&mut more),
        (Value::List(list), other) => list.push(other),
        (Value::Map(map), Value::Map(mut more)) => map.append(&mut more),
        (existing, other) => *existing = other,
    }
}

fn is_map_field(pool: &DescriptorPool, field: &FieldDescriptorProto) -> bool {
    field.label == Some(Label::Repeated)
        && field.r#type == Some(FieldType::Message)
        && field
            .type_name
            .as_deref()
            .and_then(|t| pool.lookup_message(t))
            .map(|d| is_map_entry(&d))
            .unwrap_or(false)
}

fn is_map_entry(desc: &DescriptorProto) -> bool {
    desc.options
        .as_deref()
        .map(|b| b.windows(2).any(|w| w == [0x38, 0x01]))
        .unwrap_or(false)
}

fn map_entry_to_value(v: Value) -> Value {
    // A parsed map entry has been decoded as a message with fields 1 (key) and 2 (value).
    if let Value::Message(dm) = &v {
        let k = dm
            .fields
            .get(&1)
            .cloned()
            .unwrap_or(Value::Scalar(RScalar::I64(0)));
        let val = dm
            .fields
            .get(&2)
            .cloned()
            .unwrap_or(Value::Scalar(RScalar::I64(0)));
        return Value::Map(vec![(k, val)]);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_proto_compiler::compile;
    use tpt_proto_language::parse_file;
    use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue as RScalar, Value};

    const SRC: &str = r#"
syntax = "proto3";
package ex;

enum Color { RED = 0; GREEN = 1; BLUE = 2; }

message Sub { int32 x = 1; string y = 2; }

message Person {
  string name = 1;
  int32 id = 2;
  repeated string emails = 3;
  map<string, int32> labels = 4;
  Color favorite = 5;
  Sub sub = 6;
}
"#;

    fn pool_person() -> (DescriptorPool, Arc<DescriptorProto>) {
        let parsed = parse_file("ex.proto", SRC);
        assert!(!parsed
            .diagnostics
            .iter()
            .any(|d| d.severity == tpt_proto_language::Severity::Error));
        let (fd, diags) = compile(&parsed.file);
        assert!(
            !diags
                .iter()
                .any(|d| d.severity == tpt_proto_language::Severity::Error),
            "diags: {:?}",
            diags
        );
        let pool = DescriptorPool::from_file(&fd);
        let m = pool.lookup_message("ex.Person").unwrap();
        (pool, m)
    }

    #[test]
    fn print_and_parse_roundtrip() {
        let (pool, m) = pool_person();
        let mut dm = DynamicMessage::new(m.clone(), pool.clone());
        dm.set_field(1, Value::Scalar(RScalar::String("Alice".into())));
        dm.set_field(2, Value::Scalar(RScalar::I64(7)));
        dm.set_field(
            3,
            Value::List(vec![
                Value::Scalar(RScalar::String("a@x".into())),
                Value::Scalar(RScalar::String("b@y".into())),
            ]),
        );
        dm.set_field(
            4,
            Value::Map(vec![(
                Value::Scalar(RScalar::String("home".into())),
                Value::Scalar(RScalar::I64(1)),
            )]),
        );
        dm.set_field(5, Value::Enum(2));
        let mut sub = DynamicMessage::new(pool.lookup_message("ex.Sub").unwrap(), pool.clone());
        sub.set_field(1, Value::Scalar(RScalar::I64(9)));
        sub.set_field(2, Value::Scalar(RScalar::String("hi".into())));
        dm.set_field(6, Value::Message(sub));

        let opts = TextOptions::default();
        let text = message_to_text(&pool, &m, &dm, &opts);
        eprintln!("TEXT:\n{text}");

        let parsed = text_to_message(&pool, &m, &text, &opts).unwrap();
        assert_eq!(
            parsed.get_field(1),
            Some(&Value::Scalar(RScalar::String("Alice".into())))
        );
        assert_eq!(parsed.get_field(2), Some(&Value::Scalar(RScalar::I64(7))));
        assert_eq!(parsed.get_field(5), Some(&Value::Enum(2)));
        let bytes1 = dm.encode().unwrap();
        let bytes2 = parsed.encode().unwrap();
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn parse_from_literal() {
        let (pool, m) = pool_person();
        let text = r#"
name: "Bob"
id: 42
emails: "c@d"
favorite: GREEN
labels {
  key: "x"
  value: 3
}
sub {
  x: 5
  y: "z"
}
"#;
        let opts = TextOptions::default();
        let dm = text_to_message(&pool, &m, text, &opts).unwrap();
        assert_eq!(
            dm.get_field(1),
            Some(&Value::Scalar(RScalar::String("Bob".into())))
        );
        assert_eq!(dm.get_field(5), Some(&Value::Enum(1)));
        match dm.get_field(4) {
            Some(Value::Map(entries)) => assert_eq!(entries.len(), 1),
            other => panic!("expected map, got {other:?}"),
        }
    }

    #[test]
    fn deterministic_ordering() {
        let src = r#"
syntax = "proto3";
package dt;
message Messy {
  string c = 3;
  string a = 1;
  string b = 2;
  map<string, int32> labels = 4;
}
"#;
        let parsed = parse_file("dt.proto", src);
        assert!(!parsed
            .diagnostics
            .iter()
            .any(|d| d.severity == tpt_proto_language::Severity::Error));
        let (fd, diags) = compile(&parsed.file);
        assert!(
            !diags
                .iter()
                .any(|d| d.severity == tpt_proto_language::Severity::Error),
            "diags: {:?}",
            diags
        );
        let pool = DescriptorPool::from_file(&fd);
        let m = pool.lookup_message("dt.Messy").unwrap();

        let mut dm = DynamicMessage::new(m.clone(), pool.clone());
        dm.set_field(3, Value::Scalar(RScalar::String("three".into())));
        dm.set_field(1, Value::Scalar(RScalar::String("one".into())));
        dm.set_field(2, Value::Scalar(RScalar::String("two".into())));
        dm.set_field(
            4,
            Value::Map(vec![
                (
                    Value::Scalar(RScalar::String("z".into())),
                    Value::Scalar(RScalar::I64(1)),
                ),
                (
                    Value::Scalar(RScalar::String("a".into())),
                    Value::Scalar(RScalar::I64(2)),
                ),
            ]),
        );

        // Non-deterministic: declaration order (c, a, b).
        let nondet = message_to_text(&pool, &m, &dm, &TextOptions::default());
        let first_nondet = nondet
            .lines()
            .next()
            .unwrap()
            .trim()
            .split(':')
            .next()
            .unwrap()
            .trim();
        assert_eq!(first_nondet, "c");

        // Deterministic: ascending field-number order (a, b, c, labels).
        let det = message_to_text(
            &pool,
            &m,
            &dm,
            &TextOptions {
                deterministic: true,
                ..Default::default()
            },
        );
        let first_det = det
            .lines()
            .next()
            .unwrap()
            .trim()
            .split(':')
            .next()
            .unwrap()
            .trim();
        assert_eq!(first_det, "a");
        // Map keys should be emitted in sorted order: "a" before "z".
        let a_idx = det.find("key: \"a\"").unwrap();
        let z_idx = det.find("key: \"z\"").unwrap();
        assert!(a_idx < z_idx);
    }
}
