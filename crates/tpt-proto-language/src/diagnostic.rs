//! Source spans, severity, and diagnostics for the proto language parser.

/// A zero-based byte offset into the source.
pub type ByteOffset = usize;

/// A source position: line/column (1-based for display) and byte offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub column: u32,
    /// 0-based byte offset.
    pub offset: ByteOffset,
}

/// A half-open span between two [`Position`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    /// Inclusive start.
    pub start: Position,
    /// Exclusive end.
    pub end: Position,
}

/// Diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Recoverable warning.
    Warning,
    /// Fatal error.
    Error,
}

/// Stable error codes for parser/compiler diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorCode {
    /// An unexpected token was found.
    UnexpectedToken,
    /// A token was expected but the input ended.
    UnexpectedEof,
    /// The `syntax` declaration is missing or invalid.
    InvalidSyntax,
    /// A duplicate definition of the same name.
    DuplicateSymbol,
    /// A field number is out of the valid range (1..=536_870_911, excl. 19_000..=19_999).
    InvalidFieldNumber,
    /// A reserved range/name conflict.
    ReservedConflict,
    /// An unknown/unsupported type reference.
    UnknownType,
    /// A malformed option name or value.
    InvalidOption,
    /// An editions feature is invalid or unsupported.
    InvalidFeature,
    /// A generic syntax error with no specific code.
    Other,
}

/// A single diagnostic emitted during parsing or analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Severity.
    pub severity: Severity,
    /// Stable error code.
    pub code: ErrorCode,
    /// Human-readable message.
    pub message: String,
    /// Span of the offending source.
    pub span: Option<Span>,
    /// Optional suggested fix text.
    pub suggestion: Option<String>,
}

impl Diagnostic {
    /// Create an error diagnostic.
    pub fn error(code: ErrorCode, message: impl Into<String>, span: Option<Span>) -> Self {
        Diagnostic {
            severity: Severity::Error,
            code,
            message: message.into(),
            span,
            suggestion: None,
        }
    }

    /// Create a warning diagnostic.
    pub fn warning(code: ErrorCode, message: impl Into<String>, span: Option<Span>) -> Self {
        Diagnostic {
            severity: Severity::Warning,
            code,
            message: message.into(),
            span,
            suggestion: None,
        }
    }

    /// Attach a suggested fix.
    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }
}

/// A list of diagnostics collected during a parse/analysis pass.
#[derive(Debug, Clone, Default)]
pub struct Diagnostics {
    items: Vec<Diagnostic>,
}

impl Diagnostics {
    /// Create an empty collection.
    pub fn new() -> Self {
        Diagnostics::default()
    }

    /// Push a diagnostic.
    pub fn push(&mut self, d: Diagnostic) {
        self.items.push(d);
    }

    /// Whether any error-severity diagnostics are present.
    pub fn has_errors(&self) -> bool {
        self.items.iter().any(|d| d.severity == Severity::Error)
    }

    /// Iterate over all diagnostics.
    pub fn iter(&self) -> impl Iterator<Item = &Diagnostic> {
        self.items.iter()
    }

    /// Number of diagnostics.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether no diagnostics were recorded.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
