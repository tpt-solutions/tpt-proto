//! `tpt-proto-text` — Protocol Buffers text format (§4.8, §13).
//!
//! Prints a descriptor-driven [`DynamicMessage`] to the protobuf text format
//! and parses text format back into a [`DynamicMessage`].

use std::sync::Arc;

use tpt_proto_descriptor::{
    DescriptorProto, FieldDescriptorProto, FieldType, Label,
};
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
}

impl Default for TextOptions {
    fn default() -> Self {
        TextOptions {
            use_braces: false,
            use_field_numbers: false,
            deterministic: false,
            max_depth: 100,
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
    let mut out = String::new();
    print_message(pool, descriptor, msg, opts, 0, &mut out);
    out
}

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
        } else {
            field.name.clone().unwrap_or_default()
        };
        if let Some(value) = msg.get_field(num) {
            print_value(pool, field, &name, value, opts, indent + 1, out);
        }
    }
    out.push_str(&"  ".repeat(indent));
    out.push_str(close);
    out.push('\n');
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
        if let Some(entry) = field.type_name.as_deref().and_then(|t| pool.lookup_message(t)) {
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
        (Value::Scalar(F64(x)), Value::Scalar(F64(y))) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        _ => Ordering::Equal,
    }
}

fn print_scalar_inline(v: &Value, out: &mut String) {
    match v {
        Value::Scalar(RScalar::String(s)) => {
            out.push_str(&format!("{:?}", s));
        }
        Value::Scalar(RScalar::Bytes(b)) => {
            out.push_str("0x");
            for byte in b {
                out.push_str(&format!("{byte:02x}"));
            }
        }
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
        _ => out.push_str("?"),
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
    let mut p = Parser { toks: tokens, pos: 0, opts };
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
    Ident(String),
    Str(String),
    Int(i64),
    Uint(u64),
    Float(f64),
    // Raw text of a numeric/ident token for enums/unknown handling.
    Raw(String),
    Colon,
    Comma,
    LBrace,
    RBrace,
    LAngle,
    RAngle,
    LBracket,
    RBracket,
}

fn tokenize(input: &str) -> Result<Vec<Tok>, TextError> {
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut toks = Vec::new();
    let n = bytes.len();
    while i < n {
        let c = bytes[i];
        match c {
            b' ' | b'\t' | b'\r' | b'\n' => {
                i += 1;
            }
            b'#' => {
                while i < n && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'{' => {
                toks.push(Tok::LBrace);
                i += 1;
            }
            b'}' => {
                toks.push(Tok::RBrace);
                i += 1;
            }
            b'<' => {
                toks.push(Tok::LAngle);
                i += 1;
            }
            b'>' => {
                toks.push(Tok::RAngle);
                i += 1;
            }
            b'[' => {
                toks.push(Tok::LBracket);
                i += 1;
            }
            b']' => {
                toks.push(Tok::RBracket);
                i += 1;
            }
            b':' => {
                toks.push(Tok::Colon);
                i += 1;
            }
            b',' => {
                toks.push(Tok::Comma);
                i += 1;
            }
            b'"' | b'\'' => {
                let quote = c;
                let (s, ni) = parse_string(input, i, quote)?;
                toks.push(Tok::Str(s));
                i = ni;
            }
            _ if is_ident_start(c) => {
                let start = i;
                while i < n && is_ident_part(bytes[i]) {
                    i += 1;
                }
                let word = &input[start..i];
                toks.push(classify_ident(word));
            }
            _ if c == b'-' || c == b'+' || c.is_ascii_digit() => {
                let (tok, ni) = parse_number(input, i)?;
                toks.push(tok);
                i = ni;
            }
            _ => {
                return Err(TextError::Lex(i as u32, format!("unexpected byte {:?}", c as char)));
            }
        }
    }
    Ok(toks)
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'.'
}

fn is_ident_part(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'.'
}

fn classify_ident(word: &str) -> Tok {
    match word {
        "true" | "false" => Tok::Raw(word.to_string()),
        "inf" | "nan" | "-inf" => Tok::Raw(word.to_string()),
        _ => Tok::Ident(word.to_string()),
    }
}

fn parse_string(input: &str, start: usize, quote: u8) -> Result<(String, usize), TextError> {
    let bytes = input.as_bytes();
    let mut i = start + 1;
    let n = bytes.len();
    let mut out = String::new();
    while i < n {
        let c = bytes[i];
        if c == quote {
            return Ok((out, i + 1));
        }
        if c == b'\\' && i + 1 < n {
            i += 1;
            let e = bytes[i];
            match e {
                b'n' => out.push('\n'),
                b't' => out.push('\t'),
                b'r' => out.push('\r'),
                b'\\' => out.push('\\'),
                b'\'' => out.push('\''),
                b'"' => out.push('"'),
                b'0' => out.push('\0'),
                b'\n' => {}
                b'x' => {
                    let h: String = input[i + 1..].chars().take(2).collect();
                    if let Ok(v) = u8::from_str_radix(&h, 16) {
                        out.push(v as char);
                        i += 2;
                    }
                }
                _ => out.push(e as char),
            }
        } else {
            let ch = input[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        i += 1;
    }
    Err(TextError::Lex(start as u32, "unterminated string".into()))
}

fn parse_number(input: &str, start: usize) -> Result<(Tok, usize), TextError> {
    let bytes = input.as_bytes();
    let n = bytes.len();
    let mut i = start;
    while i < n {
        let c = bytes[i];
        if c.is_ascii_digit() || c == b'.' || c == b'-' || c == b'+' || c == b'e' || c == b'E' || c == b'x' || c == b'a' || c == b'b' || c == b'c' || c == b'd' || c == b'f' || c == b'A' || c == b'B' || c == b'C' || c == b'D' || c == b'F' {
            i += 1;
        } else {
            break;
        }
    }
    let word = &input[start..i];
    // Hex
    if let Some(hex) = word.strip_prefix("0x").or_else(|| word.strip_prefix("0X")) {
        if let Ok(v) = u64::from_str_radix(hex, 16) {
            return Ok((Tok::Uint(v), i));
        }
    }
    if word.contains('.') || word.contains('e') || word.contains('E') {
        if let Ok(f) = word.parse::<f64>() {
            return Ok((Tok::Float(f), i));
        }
    }
    if let Ok(u) = word.parse::<u64>() {
        return Ok((Tok::Uint(u), i));
    }
    if let Ok(s) = word.parse::<i64>() {
        return Ok((Tok::Int(s), i));
    }
    Err(TextError::Lex(start as u32, format!("bad number {word:?}")))
}

struct Parser<'a> {
    toks: Vec<Tok>,
    pos: usize,
    opts: &'a TextOptions,
}

impl Parser<'_> {
    fn peek(&self) -> Option<Tok> {
        self.toks.get(self.pos).cloned()
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    fn expect(&mut self, t: &Tok) -> Result<(), TextError> {
        match self.next() {
            Some(ref x) if x == t => Ok(()),
            other => Err(TextError::Syntax(format!("expected {:?}, got {:?}", t, other))),
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
            match self.peek() {
                None | Some(Tok::RBrace) | Some(Tok::RAngle) => {
                    return Ok(());
                }
                Some(Tok::LBracket) => {
                    // [type_name] { ... } -> treat as nested message by type name (e.g. Any)
                    self.pos += 1;
                    let type_name = match self.next() {
                        Some(Tok::Ident(s)) => s,
                        _ => return Err(TextError::Syntax("expected type name in [ ]".into())),
                    };
                    self.expect(&Tok::RBracket)?;
                    // Determine target: either an extension or Any-like; we look up the message
                    // and nest its body into a field named by the type if present.
                    let sub = pool.lookup_message(&type_name).ok_or_else(|| TextError::Unknown(type_name.clone()))?;
                    let mut inner = DynamicMessage::new(sub.clone(), pool.clone());
                    match self.peek() {
                        Some(Tok::LBrace) | Some(Tok::LAngle) => {
                            self.next();
                            self.parse_message_body(pool, &sub, &mut inner, depth + 1)?;
                            self.close_delim()?;
                        }
                        _ => {}
                    }
                    if let Some(f) = descriptor.field.iter().find(|f| f.type_name.as_deref() == Some(&format!(".{type_name}")) || f.type_name.as_deref() == Some(type_name.as_str())) {
                        msg.set_field(f.number.unwrap_or(0), Value::Message(inner));
                    }
                }
                Some(Tok::Ident(name)) => {
                    self.pos += 1;
                    self.parse_field(pool, descriptor, msg, &name, depth)?;
                }
                Some(Tok::Raw(name)) => {
                    self.pos += 1;
                    self.parse_field(pool, descriptor, msg, &name, depth)?;
                }
                Some(other) => {
                    return Err(TextError::Syntax(format!("unexpected token in message body: {other:?}")));
                }
            }
        }
    }

    fn close_delim(&mut self) -> Result<(), TextError> {
        match self.next() {
            Some(Tok::RBrace) | Some(Tok::RAngle) => Ok(()),
            other => Err(TextError::Syntax(format!("expected closing delimiter, got {other:?}"))),
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
            .find(|f| f.name.as_deref() == Some(name) || f.number == name.parse::<i32>().ok())
            .cloned()
            .ok_or_else(|| TextError::Unknown(name.to_string()))?;

        // Determine value form: `name: value` or `name { ... }` (message) or `name: { ... }`.
        let value = match self.peek() {
            Some(Tok::LBrace) | Some(Tok::LAngle) => {
                self.next();
                let sub = pool
                    .lookup_message(field.type_name.as_deref().unwrap_or(""))
                    .unwrap_or_else(|| Arc::new(DescriptorProto::default()));
                let mut inner = DynamicMessage::new(sub.clone(), pool.clone());
                        self.parse_message_body(pool, &sub, &mut inner, depth + 1)?;
                self.close_delim()?;
                Value::Message(inner)
            }
            Some(Tok::Colon) => {
                self.next();
                self.parse_value(pool, &field, depth + 1)?
            }
            other => {
                return Err(TextError::Syntax(format!("expected ':' or '{{' after field {name}, got {other:?}")));
            }
        };

        let num = field.number.unwrap_or(0);
        if is_map_field(pool, &field) {
            if let Value::Map(mut entries) = map_entry_to_value(value) {
                if let Some((k, v)) = entries.pop() {
                    msg.fields.entry(num).or_insert_with(|| Value::Map(Vec::new()));
                    if let Some(Value::Map(m)) = msg.fields.get_mut(&num) {
                        m.push((k, v));
                    }
                }
            }
        } else if field.label == Some(Label::Repeated) {
            let entry = msg.fields.entry(num).or_insert_with(|| Value::List(Vec::new()));
            match entry {
                Value::List(l) => {
                    match value {
                        Value::List(mut inner) => l.append(&mut inner),
                        other => l.push(other),
                    }
                }
                _ => *entry = value,
            }
        } else {
            msg.set_field(field.number.unwrap_or(0), value);
        }
        // Optional trailing comma.
        if matches!(self.peek(), Some(Tok::Comma)) {
            self.next();
        }
        Ok(())
    }

    fn parse_value(&mut self, pool: &DescriptorPool, field: &FieldDescriptorProto, depth: u32) -> Result<Value, TextError> {
        let t = field.r#type.unwrap_or(FieldType::String);
        match t {
            FieldType::Message | FieldType::Group => {
                // `field: { ... }` form
                match self.peek() {
                    Some(Tok::LBrace) | Some(Tok::LAngle) => {
                        self.next();
                        let sub = pool
                            .lookup_message(field.type_name.as_deref().unwrap_or(""))
                            .unwrap_or_else(|| Arc::new(DescriptorProto::default()));
                        let mut inner = DynamicMessage::new(sub.clone(), pool.clone());
                self.parse_message_body(pool, &sub, &mut inner, depth + 1)?;
                        self.close_delim()?;
                        Ok(Value::Message(inner))
                    }
                    _ => Err(TextError::Syntax("expected message body".into())),
                }
            }
            FieldType::Enum => {
                let tok = self.next().ok_or_else(|| TextError::Syntax("expected enum value".into()))?;
                let name = match tok {
                    Tok::Ident(s) | Tok::Raw(s) => s,
                    Tok::Int(n) => return Ok(Value::Enum(n as i32)),
                    Tok::Uint(n) => return Ok(Value::Enum(n as i32)),
                    _ => return Err(TextError::Value(field.name.clone().unwrap_or_default(), "expected enum".into())),
                };
                let edesc = pool
                    .lookup_enum(field.type_name.as_deref().unwrap_or(""))
                    .ok_or_else(|| TextError::Unknown(field.type_name.clone().unwrap_or_default()))?;
                if let Some(v) = edesc.value.iter().find(|v| v.name.as_deref() == Some(&name)) {
                    Ok(Value::Enum(v.number.unwrap_or(0)))
                } else if let Ok(n) = name.parse::<i32>() {
                    Ok(Value::Enum(n))
                } else {
                    Err(TextError::Value(field.name.clone().unwrap_or_default(), format!("unknown enum value {name}")))
                }
            }
            // Scalars.
            _ => {
                let tok = self.next().ok_or_else(|| TextError::Syntax("expected value".into()))?;
                let sv = match tok {
                    Tok::Str(s) => match t {
                        FieldType::String => RScalar::String(s),
                        FieldType::Bytes => RScalar::Bytes(parse_hex(&s)),
                        _ => return Err(TextError::Value(field.name.clone().unwrap_or_default(), "unexpected string".into())),
                    },
                    Tok::Int(n) => int_to_scalar(t, n, field)?,
                    Tok::Uint(n) => uint_to_scalar(t, n, field)?,
                    Tok::Float(f) => RScalar::F64(f),
                    Tok::Raw(s) => match s.as_str() {
                        "true" => RScalar::Bool(true),
                        "false" => RScalar::Bool(false),
                        "inf" => RScalar::F64(f64::INFINITY),
                        "-inf" => RScalar::F64(f64::NEG_INFINITY),
                        "nan" => RScalar::F64(f64::NAN),
                        _ => return Err(TextError::Value(field.name.clone().unwrap_or_default(), format!("bad literal {s}"))),
                    },
                    _ => return Err(TextError::Value(field.name.clone().unwrap_or_default(), "unexpected token".into())),
                };
                Ok(Value::Scalar(sv))
            }
        }
    }
}

fn int_to_scalar(t: FieldType, n: i64, field: &FieldDescriptorProto) -> Result<RScalar, TextError> {
    use RScalar::*;
    Ok(match t {
        FieldType::Int32 | FieldType::Sint32 | FieldType::Sfixed32 => I64(n as i32 as i64),
        FieldType::Int64 | FieldType::Sint64 | FieldType::Sfixed64 => I64(n),
        FieldType::Uint32 | FieldType::Fixed32 => U64(n as u32 as u64),
        FieldType::Uint64 | FieldType::Fixed64 => U64(n as u64),
        FieldType::Bool => Bool(n != 0),
        FieldType::Double | FieldType::Float => F64(n as f64),
        _ => return Err(TextError::Value(field.name.clone().unwrap_or_default(), "int not allowed".into())),
    })
}

fn uint_to_scalar(t: FieldType, n: u64, field: &FieldDescriptorProto) -> Result<RScalar, TextError> {
    use RScalar::*;
    Ok(match t {
        FieldType::Int32 | FieldType::Sint32 | FieldType::Sfixed32 => I64(n as i32 as i64),
        FieldType::Int64 | FieldType::Sint64 | FieldType::Sfixed64 => I64(n as i64),
        FieldType::Uint32 | FieldType::Fixed32 => U64(n as u32 as u64),
        FieldType::Uint64 | FieldType::Fixed64 => U64(n),
        FieldType::Bool => Bool(n != 0),
        FieldType::Double | FieldType::Float => F64(n as f64),
        _ => return Err(TextError::Value(field.name.clone().unwrap_or_default(), "uint not allowed".into())),
    })
}

fn parse_hex(s: &str) -> Vec<u8> {
    let s = s.trim_start_matches("0x").trim_start_matches("0X");
    let s = if s.len() % 2 == 0 { s.to_string() } else { format!("0{s}") };
    (0..s.len()).step_by(2).filter_map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
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
        let k = dm.fields.get(&1).cloned().unwrap_or(Value::Scalar(RScalar::I64(0)));
        let val = dm.fields.get(&2).cloned().unwrap_or(Value::Scalar(RScalar::I64(0)));
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
        assert!(!parsed.diagnostics.iter().any(|d| d.severity == tpt_proto_language::Severity::Error));
        let (fd, diags) = compile(&parsed.file);
        assert!(!diags.iter().any(|d| d.severity == tpt_proto_language::Severity::Error), "diags: {:?}", diags);
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
        dm.set_field(3, Value::List(vec![
            Value::Scalar(RScalar::String("a@x".into())),
            Value::Scalar(RScalar::String("b@y".into())),
        ]));
        dm.set_field(4, Value::Map(vec![(
            Value::Scalar(RScalar::String("home".into())),
            Value::Scalar(RScalar::I64(1)),
        )]));
        dm.set_field(5, Value::Enum(2));
        let mut sub = DynamicMessage::new(pool.lookup_message("ex.Sub").unwrap(), pool.clone());
        sub.set_field(1, Value::Scalar(RScalar::I64(9)));
        sub.set_field(2, Value::Scalar(RScalar::String("hi".into())));
        dm.set_field(6, Value::Message(sub));

        let opts = TextOptions::default();
        let text = message_to_text(&pool, &m, &dm, &opts);
        eprintln!("TEXT:\n{text}");

        let parsed = text_to_message(&pool, &m, &text, &opts).unwrap();
        assert_eq!(parsed.get_field(1), Some(&Value::Scalar(RScalar::String("Alice".into()))));
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
        assert_eq!(dm.get_field(1), Some(&Value::Scalar(RScalar::String("Bob".into()))));
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
        assert!(!parsed.diagnostics.iter().any(|d| d.severity == tpt_proto_language::Severity::Error));
        let (fd, diags) = compile(&parsed.file);
        assert!(!diags.iter().any(|d| d.severity == tpt_proto_language::Severity::Error), "diags: {:?}", diags);
        let pool = DescriptorPool::from_file(&fd);
        let m = pool.lookup_message("dt.Messy").unwrap();

        let mut dm = DynamicMessage::new(m.clone(), pool.clone());
        dm.set_field(3, Value::Scalar(RScalar::String("three".into())));
        dm.set_field(1, Value::Scalar(RScalar::String("one".into())));
        dm.set_field(2, Value::Scalar(RScalar::String("two".into())));
        dm.set_field(4, Value::Map(vec![
            (Value::Scalar(RScalar::String("z".into())), Value::Scalar(RScalar::I64(1))),
            (Value::Scalar(RScalar::String("a".into())), Value::Scalar(RScalar::I64(2))),
        ]));

        // Non-deterministic: declaration order (c, a, b).
        let nondet = message_to_text(&pool, &m, &dm, &TextOptions::default());
        let first_nondet = nondet.lines().nth(1).unwrap().trim().split(':').next().unwrap().trim();
        assert_eq!(first_nondet, "c");

        // Deterministic: ascending field-number order (a, b, c, labels).
        let det = message_to_text(
            &pool,
            &m,
            &dm,
            &TextOptions { deterministic: true, ..Default::default() },
        );
        let first_det = det.lines().nth(1).unwrap().trim().split(':').next().unwrap().trim();
        assert_eq!(first_det, "a");
        // Map keys should be emitted in sorted order: "a" before "z".
        let a_idx = det.find("key: \"a\"").unwrap();
        let z_idx = det.find("key: \"z\"").unwrap();
        assert!(a_idx < z_idx);
    }
}
