//! Lexer for the proto language (tokens, comments, source spans).

use crate::diagnostic::{ErrorCode, Position, Span};

/// A lexical token with its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// Token kind and payload.
    pub kind: TokenKind,
    /// Source span.
    pub span: Span,
}

/// Token kinds.
#[derive(Debug, Clone)]
pub enum TokenKind {
    /// Identifier (including keywords, which are matched by text).
    Ident(String),
    /// Integer literal (decimal/hex/octal).
    Int(i64),
    /// Floating-point literal.
    Float(f64),
    /// String literal (escapes resolved).
    Str(String),
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `,`
    Comma,
    /// `;`
    Semicolon,
    /// `.`
    Dot,
    /// `=`
    Equals,
    /// `:`
    Colon,
}

/// Errors produced by the lexer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    /// Error code.
    pub code: ErrorCode,
    /// Message.
    pub message: String,
    /// Position of the error.
    pub pos: Position,
}

struct Lexer<'a> {
    bytes: &'a [u8],
    src: &'a str,
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        Lexer {
            bytes: src.as_bytes(),
            src,
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.bytes.get(self.pos).copied()?;
        self.pos += 1;
        if b == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(b)
    }

    fn pos_of(&self, offset: usize) -> Position {
        let (line, col) = self.line_col_at(offset);
        Position {
            line,
            column: col,
            offset,
        }
    }

    /// Compute line/column for an absolute offset by scanning from start.
    fn line_col_at(&self, offset: usize) -> (u32, u32) {
        let mut line = 1u32;
        let mut col = 1u32;
        for &b in &self.bytes[..offset.min(self.bytes.len())] {
            if b == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    fn span(&self, start: Position, end_offset: usize) -> Span {
        Span {
            start,
            end: self.pos_of(end_offset),
        }
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n') => {
                    self.bump();
                }
                Some(b'/') if self.bytes.get(self.pos + 1) == Some(&b'/') => {
                    while let Some(b) = self.peek() {
                        if b == b'\n' {
                            break;
                        }
                        self.bump();
                    }
                }
                Some(b'/') if self.bytes.get(self.pos + 1) == Some(&b'*') => {
                    self.bump();
                    self.bump();
                    let mut depth = 1usize;
                    while depth > 0 {
                        match self.peek() {
                            None => break,
                            Some(b'*') if self.bytes.get(self.pos + 1) == Some(&b'/') => {
                                self.bump();
                                self.bump();
                                depth -= 1;
                            }
                            Some(b'/') if self.bytes.get(self.pos + 1) == Some(&b'*') => {
                                self.bump();
                                self.bump();
                                depth += 1;
                            }
                            Some(_) => {
                                self.bump();
                            }
                        }
                    }
                }
                _ => break,
            }
        }
    }

    fn next_token(&mut self) -> Result<Option<Token>, LexError> {
        self.skip_trivia();
        let start = self.pos_of(self.pos);
        let Some(b) = self.peek() else {
            return Ok(None);
        };
        let kind = match b {
            b'{' => self.simple(TokenKind::LBrace),
            b'}' => self.simple(TokenKind::RBrace),
            b'(' => self.simple(TokenKind::LParen),
            b')' => self.simple(TokenKind::RParen),
            b'[' => self.simple(TokenKind::LBracket),
            b']' => self.simple(TokenKind::RBracket),
            b'<' => self.simple(TokenKind::Lt),
            b'>' => self.simple(TokenKind::Gt),
            b',' => self.simple(TokenKind::Comma),
            b';' => self.simple(TokenKind::Semicolon),
            b'.' => self.simple(TokenKind::Dot),
            b'=' => self.simple(TokenKind::Equals),
            b':' => self.simple(TokenKind::Colon),
            b'"' | b'\'' => {
                let s = self.lex_string(b)?;
                TokenKind::Str(s)
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let name = self.lex_ident();
                TokenKind::Ident(name)
            }
            c if c.is_ascii_digit() => self.lex_number()?,
            _ => {
                return Err(LexError {
                    code: ErrorCode::UnexpectedToken,
                    message: format!("unexpected character '{}'", b as char),
                    pos: start,
                });
            }
        };
        let end = self.pos;
        Ok(Some(Token {
            kind,
            span: self.span(start, end),
        }))
    }

    fn simple(&mut self, kind: TokenKind) -> TokenKind {
        self.bump();
        kind
    }

    fn lex_ident(&mut self) -> String {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b.is_ascii_alphanumeric() || b == b'_' {
                self.bump();
            } else {
                break;
            }
        }
        self.src[start..self.pos].to_string()
    }

    fn lex_number(&mut self) -> Result<TokenKind, LexError> {
        let start = self.pos;
        let is_hex = self.peek() == Some(b'0') && matches!(self.bytes.get(self.pos + 1), Some(b'x') | Some(b'X'));
        if is_hex {
            self.bump();
            self.bump();
            while let Some(b) = self.peek() {
                if b.is_ascii_hexdigit() || b == b'_' {
                    self.bump();
                } else {
                    break;
                }
            }
            let text = self.src[start..self.pos].replace('_', "");
            let v = i64::from_str_radix(&text[2..], 16).map_err(|_| LexError {
                code: ErrorCode::UnexpectedToken,
                message: "invalid hexadecimal literal".into(),
                pos: self.pos_of(start),
            })?;
            return Ok(TokenKind::Int(v));
        }
        let mut saw_dot = false;
        let mut saw_exp = false;
        while let Some(b) = self.peek() {
            match b {
                b'0'..=b'9' | b'_' => {
                    self.bump();
                }
                b'.' if !saw_dot && !saw_exp => {
                    saw_dot = true;
                    self.bump();
                }
                b'e' | b'E' if !saw_exp => {
                    saw_exp = true;
                    self.bump();
                    if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                        self.bump();
                    }
                }
                _ => break,
            }
        }
        let text = self.src[start..self.pos].replace('_', "");
        if saw_dot || saw_exp {
            let v: f64 = text.parse().map_err(|_| LexError {
                code: ErrorCode::UnexpectedToken,
                message: "invalid floating-point literal".into(),
                pos: self.pos_of(start),
            })?;
            Ok(TokenKind::Float(v))
        } else {
            let v: i64 = text.parse().map_err(|_| LexError {
                code: ErrorCode::UnexpectedToken,
                message: "invalid integer literal".into(),
                pos: self.pos_of(start),
            })?;
            Ok(TokenKind::Int(v))
        }
    }

    fn lex_string(&mut self, quote: u8) -> Result<String, LexError> {
        let start = self.pos_of(self.pos);
        self.bump(); // opening quote
        let mut out = String::new();
        loop {
            match self.bump() {
                None => {
                    return Err(LexError {
                        code: ErrorCode::UnexpectedEof,
                        message: "unterminated string literal".into(),
                        pos: start,
                    });
                }
                Some(c) if c == quote => break,
                Some(b'\\') => {
                    let esc = self.bump().ok_or(LexError {
                        code: ErrorCode::UnexpectedEof,
                        message: "unterminated escape".into(),
                        pos: start,
                    })?;
                    match esc {
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        b'\\' => out.push('\\'),
                        b'\'' => out.push('\''),
                        b'"' => out.push('"'),
                        b'0' => out.push('\0'),
                        b'a' => out.push('\u{0007}'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000C}'),
                        b'v' => out.push('\u{000B}'),
                        b'x' => {
                            let h = self.read_hex(2)?;
                            out.push(char::from_u32(h).unwrap_or('\u{FFFD}'));
                        }
                        b'u' => {
                            let h = self.read_hex(4)?;
                            out.push(char::from_u32(h).ok_or(LexError {
                                code: ErrorCode::UnexpectedToken,
                                message: "invalid unicode escape".into(),
                                pos: start,
                            })?);
                        }
                        b'U' => {
                            let h = self.read_hex(8)?;
                            out.push(char::from_u32(h).ok_or(LexError {
                                code: ErrorCode::UnexpectedToken,
                                message: "invalid unicode escape".into(),
                                pos: start,
                            })?);
                        }
                        _ => {
                            return Err(LexError {
                                code: ErrorCode::UnexpectedToken,
                                message: format!("invalid escape '\\{}'", esc as char),
                                pos: start,
                            });
                        }
                    }
                }
                Some(c) => out.push(c as char),
            }
        }
        Ok(out)
    }

    fn read_hex(&mut self, n: usize) -> Result<u32, LexError> {
        let start = self.pos_of(self.pos);
        let mut v = 0u32;
        for _ in 0..n {
            let b = self.bump().ok_or(LexError {
                code: ErrorCode::UnexpectedEof,
                message: "truncated escape".into(),
                pos: start,
            })?;
            let d = (b as char).to_digit(16).ok_or(LexError {
                code: ErrorCode::UnexpectedToken,
                message: "invalid hex digit".into(),
                pos: start,
            })?;
            v = v * 16 + d;
        }
        Ok(v)
    }
}

/// Lex the entire source into a token vector, or return the first lex error.
pub fn lex(src: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer::new(src);
    let mut out = Vec::new();
    while let Some(tok) = lexer.next_token()? {
        out.push(tok);
    }
    Ok(out)
}
