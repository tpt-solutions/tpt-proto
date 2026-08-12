//! `tpt-proto-language` — lexer and parser for the Protocol Buffers language.
//!
//! Parses proto2, proto3, and editions syntax into a typed [`ast::File`].

pub mod ast;
pub mod diagnostic;
mod lexer;
mod parser;

pub use diagnostic::{Diagnostic, Diagnostics, ErrorCode, Position, Severity, Span};
pub use lexer::{lex, LexError, Token, TokenKind};
pub use parser::{parse_file, ParseResult};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::ScalarType;

    const PROTO3: &str = r#"
syntax = "proto3";
package example;

import public "other.proto";

// a message
message Person {
  string name = 1;
  int32 id = 2;
  repeated string emails = 3;
  optional Address address = 4;

  enum PhoneType {
    MOBILE = 0;
    HOME = 1;
    WORK = 2;
  }

  message Address {
    string city = 1;
  }

  oneof contact {
    string email = 5;
    string phone = 6;
  }

  map<string, int32> labels = 7;
}

service Directory {
  rpc Lookup(Person) returns (Person);
  rpc Stream(stream Person) returns (stream Person);
}
"#;

    #[test]
    fn parse_proto3_basic() {
        let r = parse_file("test.proto", PROTO3);
        assert!(!r.diagnostics.has_errors(), "unexpected diagnostics: {:?}", r.diagnostics.iter().collect::<Vec<_>>());
        let f = r.file;
        assert_eq!(f.syntax.as_ref().unwrap().value, "proto3");
        assert_eq!(f.package.as_ref().unwrap().name, "example");
        assert_eq!(f.imports.len(), 1);
        assert_eq!(f.imports[0].kind, ast::ImportKind::Public);
        assert_eq!(f.messages.len(), 1);
        let person = &f.messages[0];
        assert_eq!(person.name.name, "Person");
        assert_eq!(person.fields.len(), 4);
        assert_eq!(person.oneofs.len(), 1);
        assert_eq!(person.maps.len(), 1);
        assert_eq!(person.nested_messages.len(), 1);
        assert_eq!(person.nested_enums.len(), 1);
        assert!(matches!(person.fields[0].ty, ast::TypeRef::Scalar(ScalarType::String)));
        assert_eq!(f.services.len(), 1);
        assert_eq!(f.services[0].methods.len(), 2);
        assert!(f.services[0].methods[1].client_streaming);
        assert!(f.services[0].methods[1].server_streaming);
    }

    #[test]
    fn parse_proto2_required_and_default() {
        let src = r#"
syntax = "proto2";
message M {
  required int32 a = 1 [default = 5];
  optional string b = 2 [json_name = "bee"];
  repeated double c = 3;
  extensions 100 to 200;
  reserved 9, 11 to 15;
  reserved "foo", "bar";
}
"#;
        let r = parse_file("p2.proto", src);
        assert!(!r.diagnostics.has_errors(), "unexpected diagnostics: {:?}", r.diagnostics.iter().collect::<Vec<_>>());
        let m = &r.file.messages[0];
        assert_eq!(m.fields[0].label, ast::Label::Required);
        assert_eq!(m.fields[0].default, Some(ast::Constant::Int(5)));
        assert_eq!(m.fields[1].json_name.as_deref(), Some("bee"));
        assert_eq!(m.extension_ranges.len(), 1);
        assert_eq!(m.reserved_ranges.len(), 2);
        assert_eq!(m.reserved_names.len(), 2);
    }

    #[test]
    fn lex_error_reported() {
        let r = parse_file("bad.proto", "message { @ }");
        assert!(r.diagnostics.has_errors());
    }

    #[test]
    fn editions_syntax_accepted() {
        let src = r#"edition = "2023"; message M { int32 x = 1; }"#;
        let r = parse_file("e.proto", src);
        assert!(!r.diagnostics.has_errors(), "diagnostics: {:?}", r.diagnostics.iter().collect::<Vec<_>>());
        assert!(r.file.syntax.is_none());
    }
}
