//! Abstract syntax tree for the proto language.

use crate::diagnostic::Span;

/// A named identifier with source location.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    /// The identifier text.
    pub name: String,
    /// Source span.
    pub span: Span,
}

/// The proto `syntax` declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Syntax {
    /// Span of the declaration.
    pub span: Span,
    /// The syntax value (e.g. `"proto3"`).
    pub value: String,
}

/// Kind of an import statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// `import "x";`
    Default,
    /// `import public "x";`
    Public,
    /// `import weak "x";`
    Weak,
}

/// An `import` statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// Span of the statement.
    pub span: Span,
    /// Imported path.
    pub path: String,
    /// Import kind.
    pub kind: ImportKind,
}

/// A field/enum/extension option assignment.
#[derive(Debug, Clone, PartialEq)]
pub struct ProtoOption {
    /// Span of the option.
    pub span: Span,
    /// Option name, possibly `(custom.option)`.
    pub name: String,
    /// Option value.
    pub value: Constant,
}

/// A scalar field type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarType {
    Double,
    Float,
    Int32,
    Int64,
    Uint32,
    Uint64,
    Sint32,
    Sint64,
    Fixed32,
    Fixed64,
    Sfixed32,
    Sfixed64,
    Bool,
    String,
    Bytes,
}

impl ScalarType {
    /// The protobuf keyword spelling of the scalar.
    pub fn as_str(self) -> &'static str {
        match self {
            ScalarType::Double => "double",
            ScalarType::Float => "float",
            ScalarType::Int32 => "int32",
            ScalarType::Int64 => "int64",
            ScalarType::Uint32 => "uint32",
            ScalarType::Uint64 => "uint64",
            ScalarType::Sint32 => "sint32",
            ScalarType::Sint64 => "sint64",
            ScalarType::Fixed32 => "fixed32",
            ScalarType::Fixed64 => "fixed64",
            ScalarType::Sfixed32 => "sfixed32",
            ScalarType::Sfixed64 => "sfixed64",
            ScalarType::Bool => "bool",
            ScalarType::String => "string",
            ScalarType::Bytes => "bytes",
        }
    }

    /// Parse a scalar keyword.
    pub fn from_keyword(s: &str) -> Option<ScalarType> {
        Some(match s {
            "double" => ScalarType::Double,
            "float" => ScalarType::Float,
            "int32" => ScalarType::Int32,
            "int64" => ScalarType::Int64,
            "uint32" => ScalarType::Uint32,
            "uint64" => ScalarType::Uint64,
            "sint32" => ScalarType::Sint32,
            "sint64" => ScalarType::Sint64,
            "fixed32" => ScalarType::Fixed32,
            "fixed64" => ScalarType::Fixed64,
            "sfixed32" => ScalarType::Sfixed32,
            "sfixed64" => ScalarType::Sfixed64,
            "bool" => ScalarType::Bool,
            "string" => ScalarType::String,
            "bytes" => ScalarType::Bytes,
            _ => return None,
        })
    }
}

/// A field or element type reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    /// A built-in scalar type.
    Scalar(ScalarType),
    /// A message or enum type (possibly qualified, e.g. `.foo.Bar`).
    Named(Ident),
}

/// Field label / cardinality.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    /// proto2 `optional`.
    Optional,
    /// proto2 `required`.
    Required,
    /// `repeated`.
    Repeated,
    /// proto3 singular (no label).
    Singular,
}

/// A numeric range `[start, end]`, where `end` is inclusive (or `None` for a
/// single value / max via `to max`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntRange {
    /// Inclusive start.
    pub start: i64,
    /// Inclusive end, or `None` for a single-element range.
    pub end: Option<i64>,
}

/// A message field.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// Span of the field.
    pub span: Span,
    /// Field name.
    pub name: Ident,
    /// Field number.
    pub number: i64,
    /// Field type.
    pub ty: TypeRef,
    /// Field label.
    pub label: Label,
    /// `json_name` (proto3) if specified.
    pub json_name: Option<String>,
    /// `default` value (proto2) if specified.
    pub default: Option<Constant>,
    /// Field options.
    pub options: Vec<ProtoOption>,
}

/// A `oneof` declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Oneof {
    /// Span of the oneof.
    pub span: Span,
    /// Oneof name.
    pub name: Ident,
    /// Fields within the oneof.
    pub fields: Vec<Field>,
    /// Oneof options.
    pub options: Vec<ProtoOption>,
}

/// A `map<K, V>` field.
#[derive(Debug, Clone, PartialEq)]
pub struct MapField {
    /// Span of the field.
    pub span: Span,
    /// Field name.
    pub name: Ident,
    /// Field number.
    pub number: i64,
    /// Map key type (must be a scalar or string/bytes).
    pub key: TypeRef,
    /// Map value type.
    pub value: TypeRef,
    /// Field options.
    pub options: Vec<ProtoOption>,
}

/// A message definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Span of the message.
    pub span: Span,
    /// Message name.
    pub name: Ident,
    /// Regular fields.
    pub fields: Vec<Field>,
    /// Oneofs.
    pub oneofs: Vec<Oneof>,
    /// Map fields.
    pub maps: Vec<MapField>,
    /// Nested messages.
    pub nested_messages: Vec<Message>,
    /// Nested enums.
    pub nested_enums: Vec<Enum>,
    /// Nested `extend` blocks.
    pub nested_extends: Vec<Extension>,
    /// Reserved number ranges.
    pub reserved_ranges: Vec<IntRange>,
    /// Reserved names.
    pub reserved_names: Vec<Ident>,
    /// Extension number ranges.
    pub extension_ranges: Vec<IntRange>,
    /// Message options.
    pub options: Vec<ProtoOption>,
    /// Whether this message was declared using legacy `group` syntax.
    pub is_group: bool,
}

/// An enum value.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumValue {
    /// Span.
    pub span: Span,
    /// Value name.
    pub name: Ident,
    /// Numeric value.
    pub number: i64,
    /// Options.
    pub options: Vec<ProtoOption>,
}

/// An enum definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Enum {
    /// Span.
    pub span: Span,
    /// Enum name.
    pub name: Ident,
    /// Values.
    pub values: Vec<EnumValue>,
    /// Reserved number ranges.
    pub reserved_ranges: Vec<IntRange>,
    /// Reserved names.
    pub reserved_names: Vec<Ident>,
    /// Options.
    pub options: Vec<ProtoOption>,
    /// `allow_alias` option presence (parsed separately for convenience).
    pub allow_alias: bool,
}

/// An RPC method.
#[derive(Debug, Clone, PartialEq)]
pub struct Method {
    /// Span.
    pub span: Span,
    /// Method name.
    pub name: Ident,
    /// Request type.
    pub input: TypeRef,
    /// Response type.
    pub output: TypeRef,
    /// `stream` request.
    pub client_streaming: bool,
    /// `stream` response.
    pub server_streaming: bool,
    /// Options.
    pub options: Vec<ProtoOption>,
}

/// A service definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Service {
    /// Span.
    pub span: Span,
    /// Service name.
    pub name: Ident,
    /// Methods.
    pub methods: Vec<Method>,
    /// Options.
    pub options: Vec<ProtoOption>,
}

/// A top-level or nested `extend` block.
#[derive(Debug, Clone, PartialEq)]
pub struct Extension {
    /// Span.
    pub span: Span,
    /// The extended message type.
    pub extendee: Ident,
    /// Extended fields.
    pub fields: Vec<Field>,
    /// Options.
    pub options: Vec<ProtoOption>,
}

/// A literal constant value (options, defaults, map keys, etc.).
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    /// Integer literal.
    Int(i64),
    /// Floating-point literal.
    Float(f64),
    /// String literal (with escapes resolved).
    String(String),
    /// Boolean literal.
    Bool(bool),
    /// Identifier (enum value, or `(type)` custom option).
    Ident(String),
    /// Aggregate `{ ... }` literal.
    Aggregate(Vec<(String, Constant)>),
    /// List `[ ... ]` literal.
    List(Vec<Constant>),
}

/// A parsed proto file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct File {
    /// File name (for diagnostics).
    pub name: String,
    /// `syntax` declaration.
    pub syntax: Option<Syntax>,
    /// `edition` declaration (editions syntax).
    pub edition: Option<String>,
    /// `package` declaration.
    pub package: Option<Ident>,
    /// Imports.
    pub imports: Vec<Import>,
    /// File-level options.
    pub options: Vec<ProtoOption>,
    /// Top-level messages.
    pub messages: Vec<Message>,
    /// Top-level enums.
    pub enums: Vec<Enum>,
    /// Top-level services.
    pub services: Vec<Service>,
    /// Top-level `extend` blocks.
    pub extensions: Vec<Extension>,
}
