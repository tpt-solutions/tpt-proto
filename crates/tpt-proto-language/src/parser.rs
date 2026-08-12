//! Recursive-descent parser producing a proto [`File`] AST.
//!
//! Supports proto2, proto3, and editions syntax. The parser records explicit
//! labels and the `syntax` value; default label / feature resolution is the
//! responsibility of the compiler (Phase 4).

use crate::ast::*;
use crate::diagnostic::{Diagnostic, Diagnostics, ErrorCode, Span};
use crate::lexer::{lex, Token, TokenKind};

/// Parse result: the file AST and any diagnostics.
#[derive(Debug, Clone)]
pub struct ParseResult {
    /// The parsed file (possibly partial if errors occurred).
    pub file: File,
    /// Diagnostics collected.
    pub diagnostics: Diagnostics,
}

/// Parse a complete proto source string.
pub fn parse_file(name: &str, src: &str) -> ParseResult {
    let tokens = match lex(src) {
        Ok(t) => t,
        Err(e) => {
            let mut d = Diagnostics::new();
            d.push(
                Diagnostic::error(ErrorCode::UnexpectedToken, e.message, Some(Span { start: e.pos, end: e.pos }))
                    .with_suggestion("check the highlighted token"),
            );
            return ParseResult {
                file: File {
                    name: name.to_string(),
                    ..Default::default()
                },
                diagnostics: d,
            };
        }
    };
    let mut p = Parser {
        name: name.to_string(),
        tokens,
        pos: 0,
        diagnostics: Diagnostics::new(),
    };
    let file = p.parse_file();
    ParseResult {
        file,
        diagnostics: p.diagnostics,
    }
}

struct Parser {
    name: String,
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: Diagnostics,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_kind(&self) -> Option<&TokenKind> {
        self.tokens.get(self.pos).map(|t| &t.kind)
    }

    fn advance(&mut self) -> Option<Token> {
        let t = self.tokens.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn ident_text(&self) -> Option<String> {
        match self.peek_kind() {
            Some(TokenKind::Ident(s)) => Some(s.clone()),
            _ => None,
        }
    }

    fn at_keyword(&self, kw: &str) -> bool {
        matches!(self.peek_kind(), Some(TokenKind::Ident(s)) if s == kw)
    }

    fn is_scalar_keyword(&self) -> bool {
        match self.peek_kind() {
            Some(TokenKind::Ident(s)) => ScalarType::from_keyword(s).is_some(),
            _ => false,
        }
    }

    fn error(&mut self, code: ErrorCode, msg: impl Into<String>, span: Option<Span>) {
        self.diagnostics.push(Diagnostic::error(code, msg, span));
    }

    fn expect(&mut self, kind: &TokenKind) -> Option<Token> {
        match self.peek_kind() {
            Some(k) if k == kind => self.advance(),
            _ => {
                let span = self.peek().map(|t| t.span);
                self.error(
                    ErrorCode::UnexpectedToken,
                    format!("expected {:?}, found {:?}", kind, self.peek_kind()),
                    span,
                );
                None
            }
        }
    }

    fn expect_ident(&mut self) -> Option<Ident> {
        let tok = self.advance()?;
        match &tok.kind {
            TokenKind::Ident(name) => Some(Ident { name: name.clone(), span: tok.span }),
            _ => {
                self.error(ErrorCode::UnexpectedToken, "expected identifier", Some(tok.span));
                None
            }
        }
    }

    /// Skip tokens until we reach a recovery point (`;` or `}`).
    fn recover(&mut self) {
        while let Some(k) = self.peek_kind() {
            match k {
                TokenKind::Semicolon | TokenKind::RBrace => break,
                _ => {
                    self.advance();
                }
            }
        }
    }

    fn parse_file(&mut self) -> File {
        let mut file = File {
            name: self.name.clone(),
            ..Default::default()
        };
        while self.peek().is_some() {
            if self.at_keyword("syntax") {
                if let Some(s) = self.parse_syntax() {
                    file.syntax = Some(s);
                }
            } else if self.at_keyword("edition") {
                self.advance();
                self.expect(&TokenKind::Equals);
                if let Some(TokenKind::Str(s)) = self.peek_kind().cloned() {
                    self.advance();
                    file.edition = Some(s);
                }
                self.expect(&TokenKind::Semicolon);
            } else if self.at_keyword("package") {
                self.advance();
                if let Some(pkg) = self.expect_ident() {
                    file.package = Some(pkg);
                }
                self.expect(&TokenKind::Semicolon);
            } else if self.at_keyword("import") {
                if let Some(imp) = self.parse_import() {
                    file.imports.push(imp);
                }
            } else if self.at_keyword("option") {
                if let Some(o) = self.parse_option_stmt() {
                    file.options.push(o);
                }
            } else if self.at_keyword("message") {
                match self.parse_message(false) {
                    Some(m) => file.messages.push(m),
                    None => self.recover(),
                }
            } else if self.at_keyword("enum") {
                match self.parse_enum() {
                    Some(e) => file.enums.push(e),
                    None => self.recover(),
                }
            } else if self.at_keyword("service") {
                match self.parse_service() {
                    Some(s) => file.services.push(s),
                    None => self.recover(),
                }
            } else if self.at_keyword("extend") {
                match self.parse_extend() {
                    Some(e) => file.extensions.push(e),
                    None => self.recover(),
                }
            } else {
                let span = self.peek().map(|t| t.span);
                let kw = self.ident_text().unwrap_or_default();
                self.error(ErrorCode::UnexpectedToken, format!("unexpected top-level token '{kw}'"), span);
                self.advance();
            }
        }
        file
    }

    fn parse_syntax(&mut self) -> Option<Syntax> {
        let start = self.peek().unwrap().span;
        self.advance(); // 'syntax'
        self.expect(&TokenKind::Equals);
        let value = match self.peek_kind() {
            Some(TokenKind::Str(s)) => {
                let v = s.clone();
                self.advance();
                v
            }
            _ => {
                self.error(ErrorCode::InvalidSyntax, "syntax value must be a string literal", self.peek().map(|t| t.span));
                return None;
            }
        };
        self.expect(&TokenKind::Semicolon);
        let end = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(start);
        Some(Syntax { span: Span { start: start.start, end: end.end }, value })
    }

    fn parse_import(&mut self) -> Option<Import> {
        let span = self.peek().unwrap().span;
        self.advance(); // 'import'
        let mut kind = ImportKind::Default;
        if self.at_keyword("public") {
            self.advance();
            kind = ImportKind::Public;
        } else if self.at_keyword("weak") {
            self.advance();
            kind = ImportKind::Weak;
        }
        let path = match self.peek_kind() {
            Some(TokenKind::Str(s)) => {
                let p = s.clone();
                self.advance();
                p
            }
            _ => {
                self.error(ErrorCode::UnexpectedToken, "import path must be a string literal", self.peek().map(|t| t.span));
                return None;
            }
        };
        self.expect(&TokenKind::Semicolon);
        let end = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(span);
        Some(Import { span: Span { start: span.start, end: end.end }, path, kind })
    }

    fn parse_option_name(&mut self) -> Option<String> {
        // option name: ident (. ident)* , or (full.type) (. ident)*
        let mut name = String::new();
        if self.peek_kind() == Some(&TokenKind::LParen) {
            name.push('(');
            self.advance();
            // read a qualified type inside parens
            while let Some(k) = self.peek_kind() {
                match k {
                    TokenKind::Ident(_) | TokenKind::Dot => {
                        if let Some(TokenKind::Ident(s)) = self.peek_kind() {
                            name.push_str(s);
                        } else if self.peek_kind() == Some(&TokenKind::Dot) {
                            name.push('.');
                        }
                        self.advance();
                    }
                    TokenKind::RParen => {
                        name.push(')');
                        self.advance();
                        break;
                    }
                    _ => break,
                }
            }
        } else {
            name = self.expect_ident()?.name;
        }
        while self.peek_kind() == Some(&TokenKind::Dot) {
            self.advance();
            name.push('.');
            if let Some(i) = self.expect_ident() {
                name.push_str(&i.name);
            }
        }
        Some(name)
    }

    fn parse_constant(&mut self) -> Option<Constant> {
        let tok = self.peek()?.clone();
        match &tok.kind {
            TokenKind::Int(v) => {
                self.advance();
                Some(Constant::Int(*v))
            }
            TokenKind::Float(v) => {
                self.advance();
                Some(Constant::Float(*v))
            }
            TokenKind::Str(s) => {
                self.advance();
                Some(Constant::String(s.clone()))
            }
            TokenKind::Ident(s) => {
                self.advance();
                match s.as_str() {
                    "true" => Some(Constant::Bool(true)),
                    "false" => Some(Constant::Bool(false)),
                    _ => Some(Constant::Ident(s.clone())),
                }
            }
            TokenKind::LBrace => {
                self.advance();
                let mut fields = Vec::new();
                while self.peek_kind() != Some(&TokenKind::RBrace) && self.peek().is_some() {
                    let key = self.parse_option_name()?;
                    // either ':' value or nested '{' aggregate
                    if self.peek_kind() == Some(&TokenKind::LBrace) {
                        let val = self.parse_constant()?;
                        fields.push((key, val));
                    } else {
                        self.expect(&TokenKind::Colon);
                        let val = self.parse_constant()?;
                        fields.push((key, val));
                        self.expect(&TokenKind::Semicolon);
                    }
                }
                self.expect(&TokenKind::RBrace);
                Some(Constant::Aggregate(fields))
            }
            TokenKind::LBracket => {
                self.advance();
                let mut items = Vec::new();
                while self.peek_kind() != Some(&TokenKind::RBracket) && self.peek().is_some() {
                    if let Some(c) = self.parse_constant() {
                        items.push(c);
                    }
                    if self.peek_kind() == Some(&TokenKind::Comma) {
                        self.advance();
                    }
                }
                self.expect(&TokenKind::RBracket);
                Some(Constant::List(items))
            }
            _ => {
                self.error(ErrorCode::InvalidOption, "expected a constant value", Some(tok.span));
                None
            }
        }
    }

    fn parse_option_stmt(&mut self) -> Option<ProtoOption> {
        let span = self.peek().unwrap().span;
        self.advance(); // 'option'
        let name = self.parse_option_name()?;
        self.expect(&TokenKind::Equals);
        let value = self.parse_constant()?;
        self.expect(&TokenKind::Semicolon);
        let end = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(span);
        Some(ProtoOption { span: Span { start: span.start, end: end.end }, name, value })
    }

    fn parse_type_ref(&mut self) -> Option<TypeRef> {
        let tok = self.peek()?.clone();
        if let TokenKind::Ident(s) = &tok.kind {
            if let Some(sc) = ScalarType::from_keyword(s) {
                self.advance();
                return Some(TypeRef::Scalar(sc));
            }
        }
        // qualified or simple name, possibly with leading dot
        let mut name = String::new();
        if self.peek_kind() == Some(&TokenKind::Dot) {
            name.push('.');
            self.advance();
        }
        let first = self.expect_ident()?;
        name.push_str(&first.name);
        while self.peek_kind() == Some(&TokenKind::Dot) {
            self.advance();
            name.push('.');
            let i = self.expect_ident()?;
            name.push_str(&i.name);
        }
        let span = Span { start: tok.span.start, end: self.tokens.get(self.pos).map(|t| t.span.end).unwrap_or(tok.span.end) };
        Some(TypeRef::Named(Ident { name, span }))
    }

    fn parse_message(&mut self, is_group: bool) -> Option<Message> {
        let span = self.peek().unwrap().span;
        self.advance(); // 'message' or 'group'
        let name = self.expect_ident()?;
        let number = if is_group {
            self.expect(&TokenKind::Equals);
            match self.peek_kind() {
                Some(TokenKind::Int(n)) => {
                    let n = *n;
                    self.advance();
                    n
                }
                _ => {
                    self.error(ErrorCode::UnexpectedToken, "group requires a field number", self.peek().map(|t| t.span));
                    0
                }
            }
        } else {
            0
        };
        self.expect(&TokenKind::LBrace);
        let mut msg = Message {
            span,
            name,
            fields: Vec::new(),
            oneofs: Vec::new(),
            maps: Vec::new(),
            nested_messages: Vec::new(),
            nested_enums: Vec::new(),
            nested_extends: Vec::new(),
            reserved_ranges: Vec::new(),
            reserved_names: Vec::new(),
            extension_ranges: Vec::new(),
            options: Vec::new(),
            is_group,
        };
        if is_group {
            let f = Field {
                span,
                name: msg.name.clone(),
                number,
                ty: TypeRef::Named(msg.name.clone()),
                label: Label::Optional,
                json_name: None,
                default: None,
                options: Vec::new(),
            };
            msg.fields.push(f);
        }
        self.parse_message_body(&mut msg);
        Some(msg)
    }

    fn parse_message_body(&mut self, msg: &mut Message) {
        while self.peek_kind() != Some(&TokenKind::RBrace) && self.peek().is_some() {
            if self.at_keyword("option") {
                if let Some(o) = self.parse_option_stmt() {
                    msg.options.push(o);
                }
            } else if self.at_keyword("message") {
                if let Some(m) = self.parse_message(false) {
                    msg.nested_messages.push(m);
                } else {
                    self.recover();
                }
            } else if self.at_keyword("enum") {
                if let Some(e) = self.parse_enum() {
                    msg.nested_enums.push(e);
                } else {
                    self.recover();
                }
            } else if self.at_keyword("oneof") {
                if let Some(o) = self.parse_oneof() {
                    msg.oneofs.push(o);
                } else {
                    self.recover();
                }
            } else if self.at_keyword("map") {
                if let Some(mf) = self.parse_map() {
                    msg.maps.push(mf);
                } else {
                    self.recover();
                }
            } else if self.at_keyword("reserved") {
                self.parse_reserved(msg);
            } else if self.at_keyword("extensions") {
                self.parse_extensions(msg);
            } else if self.at_keyword("extend") {
                if let Some(e) = self.parse_extend() {
                    msg.nested_extends.push(e);
                } else {
                    self.recover();
                }
            } else if self.at_keyword("group") {
                if let Some(g) = self.parse_message(true) {
                    msg.nested_messages.push(g);
                } else {
                    self.recover();
                }
            } else if self.is_scalar_keyword() || self.peek_kind() == Some(&TokenKind::Dot) || matches!(self.peek_kind(), Some(TokenKind::Ident(_))) {
                if let Some(f) = self.parse_field() {
                    msg.fields.push(f);
                } else {
                    self.recover();
                }
            } else {
                let span = self.peek().map(|t| t.span);
                let kw = self.ident_text().unwrap_or_default();
                self.error(ErrorCode::UnexpectedToken, format!("unexpected token in message body: '{kw}'"), span);
                self.advance();
            }
        }
        self.expect(&TokenKind::RBrace);
    }

    fn parse_label(&mut self) -> Label {
        if self.at_keyword("optional") {
            self.advance();
            Label::Optional
        } else if self.at_keyword("required") {
            self.advance();
            Label::Required
        } else if self.at_keyword("repeated") {
            self.advance();
            Label::Repeated
        } else {
            Label::Singular
        }
    }

    fn parse_field(&mut self) -> Option<Field> {
        let span = self.peek().unwrap().span;
        let label = self.parse_label();
        let ty = self.parse_type_ref()?;
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Equals);
        let number = match self.peek_kind() {
            Some(TokenKind::Int(n)) => {
                let n = *n;
                self.advance();
                n
            }
            _ => {
                self.error(ErrorCode::UnexpectedToken, "expected field number", self.peek().map(|t| t.span));
                return None;
            }
        };
        let mut json_name = None;
        let mut default = None;
        let mut options = Vec::new();
        if self.peek_kind() == Some(&TokenKind::LBracket) {
            self.advance();
            while self.peek_kind() != Some(&TokenKind::RBracket) && self.peek().is_some() {
                if let Some(o) = self.parse_field_option(&mut json_name, &mut default) {
                    options.push(o);
                }
                if self.peek_kind() == Some(&TokenKind::Comma) {
                    self.advance();
                }
            }
            self.expect(&TokenKind::RBracket);
        }
        self.expect(&TokenKind::Semicolon);
        let end = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(span);
        Some(Field {
            span: Span { start: span.start, end: end.end },
            name,
            number,
            ty,
            label,
            json_name,
            default,
            options,
        })
    }

    fn parse_field_option(&mut self, json_name: &mut Option<String>, default: &mut Option<Constant>) -> Option<ProtoOption> {
        let name = self.parse_option_name()?;
        if name == "json_name" {
            self.expect(&TokenKind::Equals);
            if let Some(TokenKind::Str(s)) = self.peek_kind().cloned() {
                *json_name = Some(s);
                self.advance();
            }
            return None;
        }
        if name == "default" {
            self.expect(&TokenKind::Equals);
            if let Some(c) = self.parse_constant() {
                *default = Some(c);
            }
            return None;
        }
        self.expect(&TokenKind::Equals);
        let value = self.parse_constant()?;
        let span = self.peek().map(|t| t.span).unwrap_or_default();
        Some(ProtoOption { span, name, value })
    }

    fn parse_oneof(&mut self) -> Option<Oneof> {
        let span = self.peek().unwrap().span;
        self.advance(); // 'oneof'
        let name = self.expect_ident()?;
        let mut options = Vec::new();
        if self.at_keyword("option") {
            if let Some(o) = self.parse_option_stmt() {
                options.push(o);
            }
        }
        self.expect(&TokenKind::LBrace);
        let mut fields = Vec::new();
        while self.peek_kind() != Some(&TokenKind::RBrace) && self.peek().is_some() {
            // oneof fields are singular; an explicit 'optional' is allowed in editions
            let label = self.parse_label();
            let ty = self.parse_type_ref()?;
            let fname = self.expect_ident()?;
            self.expect(&TokenKind::Equals);
            let number = match self.peek_kind() {
                Some(TokenKind::Int(n)) => {
                    let n = *n;
                    self.advance();
                    n
                }
                _ => {
                    self.error(ErrorCode::UnexpectedToken, "expected field number", self.peek().map(|t| t.span));
                    return None;
                }
            };
            let mut field_options = Vec::new();
            if self.peek_kind() == Some(&TokenKind::LBracket) {
                self.advance();
                while self.peek_kind() != Some(&TokenKind::RBracket) && self.peek().is_some() {
                    if let Some(o) = self.parse_field_option(&mut None, &mut None) {
                        field_options.push(o);
                    }
                    if self.peek_kind() == Some(&TokenKind::Comma) {
                        self.advance();
                    }
                }
                self.expect(&TokenKind::RBracket);
            }
            self.expect(&TokenKind::Semicolon);
            let end = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(span);
            fields.push(Field {
                span: Span { start: span.start, end: end.end },
                name: fname,
                number,
                ty,
                label,
                json_name: None,
                default: None,
                options: field_options,
            });
        }
        self.expect(&TokenKind::RBrace);
        let end = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(span);
        Some(Oneof { span: Span { start: span.start, end: end.end }, name, fields, options })
    }

    fn parse_map(&mut self) -> Option<MapField> {
        let span = self.peek().unwrap().span;
        self.advance(); // 'map'
        self.expect(&TokenKind::Lt);
        let key = self.parse_type_ref()?;
        self.expect(&TokenKind::Comma);
        let value = self.parse_type_ref()?;
        self.expect(&TokenKind::Gt);
        let name = self.expect_ident()?;
        self.expect(&TokenKind::Equals);
        let number = match self.peek_kind() {
            Some(TokenKind::Int(n)) => {
                let n = *n;
                self.advance();
                n
            }
            _ => {
                self.error(ErrorCode::UnexpectedToken, "expected field number", self.peek().map(|t| t.span));
                return None;
            }
        };
        let mut options = Vec::new();
        if self.peek_kind() == Some(&TokenKind::LBracket) {
            self.advance();
            while self.peek_kind() != Some(&TokenKind::RBracket) && self.peek().is_some() {
                if let Some(o) = self.parse_field_option(&mut None, &mut None) {
                    options.push(o);
                }
                if self.peek_kind() == Some(&TokenKind::Comma) {
                    self.advance();
                }
            }
            self.expect(&TokenKind::RBracket);
        }
        self.expect(&TokenKind::Semicolon);
        let end = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(span);
        Some(MapField { span: Span { start: span.start, end: end.end }, name, number, key, value, options })
    }

    fn parse_ranges(&mut self) -> Vec<IntRange> {
        let mut ranges = Vec::new();
        loop {
            let start = match self.peek_kind() {
                Some(TokenKind::Int(n)) => {
                    let n = *n;
                    self.advance();
                    n
                }
                _ => {
                    self.error(ErrorCode::UnexpectedToken, "expected integer range bound", self.peek().map(|t| t.span));
                    break;
                }
            };
            let mut end = None;
            if self.at_keyword("to") {
                self.advance();
                if self.at_keyword("max") {
                    self.advance();
                    end = Some(i64::MAX);
                } else if let Some(TokenKind::Int(n)) = self.peek_kind() {
                    let n = *n;
                    self.advance();
                    end = Some(n);
                }
            }
            ranges.push(IntRange { start, end });
            if self.peek_kind() == Some(&TokenKind::Comma) {
                self.advance();
                continue;
            }
            break;
        }
        ranges
    }

    fn parse_reserved(&mut self, msg: &mut Message) {
        self.advance(); // 'reserved'
        // Could be names (strings) or number ranges.
        if let Some(TokenKind::Str(_)) = self.peek_kind() {
            while let Some(TokenKind::Str(s)) = self.peek_kind().cloned() {
                self.advance();
                msg.reserved_names.push(Ident { name: s, span: self.peek().map(|t| t.span).unwrap_or_default() });
                if self.peek_kind() == Some(&TokenKind::Comma) {
                    self.advance();
                } else {
                    break;
                }
            }
        } else {
            msg.reserved_ranges = self.parse_ranges();
        }
        self.expect(&TokenKind::Semicolon);
    }

    fn parse_extensions(&mut self, msg: &mut Message) {
        self.advance(); // 'extensions'
        msg.extension_ranges = self.parse_ranges();
        self.expect(&TokenKind::Semicolon);
    }

    fn parse_enum(&mut self) -> Option<Enum> {
        let span = self.peek().unwrap().span;
        self.advance(); // 'enum'
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LBrace);
        let mut en = Enum {
            span,
            name,
            values: Vec::new(),
            reserved_ranges: Vec::new(),
            reserved_names: Vec::new(),
            options: Vec::new(),
            allow_alias: false,
        };
        while self.peek_kind() != Some(&TokenKind::RBrace) && self.peek().is_some() {
            if self.at_keyword("option") {
                if let Some(o) = self.parse_option_stmt() {
                    if o.name == "allow_alias" {
                        if let Constant::Bool(true) = o.value {
                            en.allow_alias = true;
                        }
                    }
                    en.options.push(o);
                }
            } else if self.at_keyword("reserved") {
                self.advance();
                if let Some(TokenKind::Str(_)) = self.peek_kind() {
                    while let Some(TokenKind::Str(s)) = self.peek_kind().cloned() {
                        self.advance();
                        en.reserved_names.push(Ident { name: s, span: Span::default() });
                        if self.peek_kind() == Some(&TokenKind::Comma) {
                            self.advance();
                        } else {
                            break;
                        }
                    }
                } else {
                    en.reserved_ranges = self.parse_ranges();
                }
                self.expect(&TokenKind::Semicolon);
            } else if let Some(TokenKind::Ident(_)) = self.peek_kind() {
                // enum value: name = number [options] ;
                let vname = self.expect_ident()?;
                self.expect(&TokenKind::Equals);
                let number = match self.peek_kind() {
                    Some(TokenKind::Int(n)) => {
                        let n = *n;
                        self.advance();
                        n
                    }
                    _ => {
                        self.error(ErrorCode::UnexpectedToken, "expected enum value number", self.peek().map(|t| t.span));
                        return None;
                    }
                };
                let mut options = Vec::new();
                if self.peek_kind() == Some(&TokenKind::LBracket) {
                    self.advance();
                    while self.peek_kind() != Some(&TokenKind::RBracket) && self.peek().is_some() {
                        if let Some(o) = self.parse_field_option(&mut None, &mut None) {
                            options.push(o);
                        }
                        if self.peek_kind() == Some(&TokenKind::Comma) {
                            self.advance();
                        }
                    }
                    self.expect(&TokenKind::RBracket);
                }
                self.expect(&TokenKind::Semicolon);
                en.values.push(EnumValue { span, name: vname, number, options });
            } else {
                let span = self.peek().map(|t| t.span);
                let kw = self.ident_text().unwrap_or_default();
                self.error(ErrorCode::UnexpectedToken, format!("unexpected token in enum body: '{kw}'"), span);
                self.advance();
            }
        }
        self.expect(&TokenKind::RBrace);
        let end = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(span);
        en.span = Span { start: span.start, end: end.end };
        Some(en)
    }

    fn parse_service(&mut self) -> Option<Service> {
        let span = self.peek().unwrap().span;
        self.advance(); // 'service'
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LBrace);
        let mut service = Service {
            span,
            name,
            methods: Vec::new(),
            options: Vec::new(),
        };
        while self.peek_kind() != Some(&TokenKind::RBrace) && self.peek().is_some() {
            if self.at_keyword("option") {
                if let Some(o) = self.parse_option_stmt() {
                    service.options.push(o);
                }
            } else if self.at_keyword("rpc") {
                if let Some(m) = self.parse_method() {
                    service.methods.push(m);
                } else {
                    self.recover();
                }
            } else {
                let span = self.peek().map(|t| t.span);
                let kw = self.ident_text().unwrap_or_default();
                self.error(ErrorCode::UnexpectedToken, format!("unexpected token in service body: '{kw}'"), span);
                self.advance();
            }
        }
        self.expect(&TokenKind::RBrace);
        let end = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(span);
        service.span = Span { start: span.start, end: end.end };
        Some(service)
    }

    fn parse_method(&mut self) -> Option<Method> {
        let span = self.peek().unwrap().span;
        self.advance(); // 'rpc'
        let name = self.expect_ident()?;
        self.expect(&TokenKind::LParen);
        let client_streaming = self.at_keyword("stream");
        if client_streaming {
            self.advance();
        }
        let input = self.parse_type_ref()?;
        self.expect(&TokenKind::RParen);
        if !self.at_keyword("returns") {
            self.error(ErrorCode::UnexpectedToken, "expected 'returns'", self.peek().map(|t| t.span));
            return None;
        }
        self.advance(); // 'returns'
        self.expect(&TokenKind::LParen);
        let server_streaming = self.at_keyword("stream");
        if server_streaming {
            self.advance();
        }
        let output = self.parse_type_ref()?;
        self.expect(&TokenKind::RParen);
        let mut options = Vec::new();
        if self.peek_kind() == Some(&TokenKind::LBrace) {
            self.advance();
            while self.peek_kind() != Some(&TokenKind::RBrace) && self.peek().is_some() {
                if let Some(o) = self.parse_option_stmt() {
                    options.push(o);
                } else {
                    self.advance();
                }
            }
            self.expect(&TokenKind::RBrace);
        } else {
            self.expect(&TokenKind::Semicolon);
        }
        let end = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(span);
        Some(Method {
            span: Span { start: span.start, end: end.end },
            name,
            input,
            output,
            client_streaming,
            server_streaming,
            options,
        })
    }

    fn parse_extend(&mut self) -> Option<Extension> {
        let span = self.peek().unwrap().span;
        self.advance(); // 'extend'
        let extendee = self.expect_ident()?;
        self.expect(&TokenKind::LBrace);
        let mut ext = Extension {
            span,
            extendee,
            fields: Vec::new(),
            options: Vec::new(),
        };
        while self.peek_kind() != Some(&TokenKind::RBrace) && self.peek().is_some() {
            if self.at_keyword("option") {
                if let Some(o) = self.parse_option_stmt() {
                    ext.options.push(o);
                }
            } else if let Some(f) = self.parse_field() {
                ext.fields.push(f);
            } else {
                self.recover();
            }
        }
        self.expect(&TokenKind::RBrace);
        let end = self.tokens.get(self.pos.saturating_sub(1)).map(|t| t.span).unwrap_or(span);
        ext.span = Span { start: span.start, end: end.end };
        Some(ext)
    }
}

impl PartialEq for TokenKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (TokenKind::Ident(a), TokenKind::Ident(b)) => a == b,
            (TokenKind::Int(a), TokenKind::Int(b)) => a == b,
            (TokenKind::Float(a), TokenKind::Float(b)) => a.to_bits() == b.to_bits(),
            (TokenKind::Str(a), TokenKind::Str(b)) => a == b,
            (TokenKind::LBrace, TokenKind::LBrace) => true,
            (TokenKind::RBrace, TokenKind::RBrace) => true,
            (TokenKind::LParen, TokenKind::LParen) => true,
            (TokenKind::RParen, TokenKind::RParen) => true,
            (TokenKind::LBracket, TokenKind::LBracket) => true,
            (TokenKind::RBracket, TokenKind::RBracket) => true,
            (TokenKind::Lt, TokenKind::Lt) => true,
            (TokenKind::Gt, TokenKind::Gt) => true,
            (TokenKind::Comma, TokenKind::Comma) => true,
            (TokenKind::Semicolon, TokenKind::Semicolon) => true,
            (TokenKind::Dot, TokenKind::Dot) => true,
            (TokenKind::Equals, TokenKind::Equals) => true,
            (TokenKind::Colon, TokenKind::Colon) => true,
            _ => false,
        }
    }
}

impl std::hash::Hash for TokenKind {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            TokenKind::Ident(s) => (0u8, s).hash(state),
            TokenKind::Int(i) => (1u8, i).hash(state),
            TokenKind::Float(f) => (2u8, f.to_bits()).hash(state),
            TokenKind::Str(s) => (3u8, s).hash(state),
            TokenKind::LBrace => 4u8.hash(state),
            TokenKind::RBrace => 5u8.hash(state),
            TokenKind::LParen => 6u8.hash(state),
            TokenKind::RParen => 7u8.hash(state),
            TokenKind::LBracket => 8u8.hash(state),
            TokenKind::RBracket => 9u8.hash(state),
            TokenKind::Lt => 10u8.hash(state),
            TokenKind::Gt => 11u8.hash(state),
            TokenKind::Comma => 12u8.hash(state),
            TokenKind::Semicolon => 13u8.hash(state),
            TokenKind::Dot => 14u8.hash(state),
            TokenKind::Equals => 15u8.hash(state),
            TokenKind::Colon => 16u8.hash(state),
        }
    }
}
