//! Error types for the tpt-proto binary wire format runtime.

use std::fmt;

/// Result alias used throughout the wire-format runtime.
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur while encoding or decoding protobuf messages.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The input buffer ended before a complete value could be read.
    UnexpectedEof,
    /// A varint exceeded the maximum 10-byte length.
    VarintTooLong,
    /// A field tag could not be decoded (zero field number, or invalid wire type).
    InvalidTag(u32),
    /// A length-delimited value declared a length longer than the remaining input.
    LengthLimitExceeded {
        /// Declared length of the field.
        len: usize,
    },
    /// A string field was not valid UTF-8.
    Utf8Error {
        /// Byte offset of the invalid sequence within the field.
        offset: usize,
    },
    /// A 32-bit value did not fit in the range expected by the schema.
    IntegerOverflow,
    /// A nested group was not closed before the end of the input.
    UnterminatedGroup(u32),
    /// A recursion/depth limit was exceeded.
    DepthLimitExceeded,
    /// A cumulative decode limit (bytes, fields, strings, ...) was exceeded.
    LimitExceeded(&'static str),
    /// A wire type was encountered that is not valid for the expected field.
    UnexpectedWireType {
        /// The wire type that was found.
        found: u8,
    },
    /// The input did not match an expected structure (e.g. malformed map entry).
    MalformedInput(&'static str),
    /// A custom error from a higher-level layer (codegen, reflection, ...).
    Custom(String),
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnexpectedEof => write!(f, "unexpected end of input buffer"),
            Error::VarintTooLong => write!(f, "varint is longer than 10 bytes"),
            Error::InvalidTag(tag) => write!(f, "invalid field tag: {tag}"),
            Error::LengthLimitExceeded { len } => {
                write!(
                    f,
                    "length-delimited field length {len} exceeds remaining input"
                )
            }
            Error::Utf8Error { offset } => {
                write!(f, "string field contained invalid UTF-8 at byte {offset}")
            }
            Error::IntegerOverflow => write!(f, "integer value did not fit in the target type"),
            Error::UnterminatedGroup(n) => write!(f, "unterminated group for field {n}"),
            Error::DepthLimitExceeded => write!(f, "maximum nesting depth exceeded"),
            Error::LimitExceeded(which) => write!(f, "decode limit exceeded: {which}"),
            Error::UnexpectedWireType { found } => write!(f, "unexpected wire type {found}"),
            Error::MalformedInput(what) => write!(f, "malformed input: {what}"),
            Error::Custom(msg) => write!(f, "{msg}"),
        }
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Self {
        Error::Utf8Error {
            offset: e.valid_up_to(),
        }
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(e: std::string::FromUtf8Error) -> Self {
        Error::Utf8Error {
            offset: e.utf8_error().valid_up_to(),
        }
    }
}
