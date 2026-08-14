//! gRPC status model, including structured status codes, rich error details
//! compatible with `google.rpc.Status`, and trailer serialization.

use anyhow::Result;
use std::fmt;

use tpt_proto_core::scalar;
use tpt_proto_core::{Message, Reader, WireType, Writer};
use tpt_proto_wkt::Any;

use crate::metadata::Metadata;

/// Standard gRPC status codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Code {
    /// Not an error; returned on success.
    Ok = 0,
    /// The operation was cancelled, typically by the caller.
    Cancelled = 1,
    /// Unknown error.
    Unknown = 2,
    /// The client supplied an invalid argument.
    InvalidArgument = 3,
    /// The deadline expired before the operation could complete.
    DeadlineExceeded = 4,
    /// Some requested entity was not found.
    NotFound = 5,
    /// The entity that a client tried to create already exists.
    AlreadyExists = 6,
    /// The caller has no permission to execute the operation.
    PermissionDenied = 7,
    /// Some resource has been exhausted.
    ResourceExhausted = 8,
    /// The system is not in a state required for the operation.
    FailedPrecondition = 9,
    /// The operation was aborted by the system.
    Aborted = 10,
    /// The operation was attempted past the valid range.
    OutOfRange = 11,
    /// The operation is not implemented or is not supported.
    Unimplemented = 12,
    /// Internal error.
    Internal = 13,
    /// The service is currently unavailable.
    Unavailable = 14,
    /// Unrecoverable data loss or corruption.
    DataLoss = 15,
    /// The request does not have valid authentication.
    Unauthenticated = 16,
}

impl Code {
    /// Map an integer code to a [`Code`], defaulting to [`Code::Unknown`].
    pub fn from_i32(v: i32) -> Code {
        match v {
            0 => Code::Ok,
            1 => Code::Cancelled,
            2 => Code::Unknown,
            3 => Code::InvalidArgument,
            4 => Code::DeadlineExceeded,
            5 => Code::NotFound,
            6 => Code::AlreadyExists,
            7 => Code::PermissionDenied,
            8 => Code::ResourceExhausted,
            9 => Code::FailedPrecondition,
            10 => Code::Aborted,
            11 => Code::OutOfRange,
            12 => Code::Unimplemented,
            13 => Code::Internal,
            14 => Code::Unavailable,
            15 => Code::DataLoss,
            16 => Code::Unauthenticated,
            _ => Code::Unknown,
        }
    }

    /// Render the numeric code.
    pub fn as_i32(self) -> i32 {
        self as i32
    }

    /// A short human-readable description of the code.
    pub fn description(self) -> &'static str {
        match self {
            Code::Ok => "ok",
            Code::Cancelled => "cancelled",
            Code::Unknown => "unknown",
            Code::InvalidArgument => "invalid argument",
            Code::DeadlineExceeded => "deadline exceeded",
            Code::NotFound => "not found",
            Code::AlreadyExists => "already exists",
            Code::PermissionDenied => "permission denied",
            Code::ResourceExhausted => "resource exhausted",
            Code::FailedPrecondition => "failed precondition",
            Code::Aborted => "aborted",
            Code::OutOfRange => "out of range",
            Code::Unimplemented => "unimplemented",
            Code::Internal => "internal error",
            Code::Unavailable => "unavailable",
            Code::DataLoss => "data loss",
            Code::Unauthenticated => "unauthenticated",
        }
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.as_i32(), self.description())
    }
}

/// A gRPC status, optionally carrying rich error details.
#[derive(Debug, Clone)]
pub struct Status {
    /// The status code.
    pub code: Code,
    /// A human-readable status message.
    pub message: String,
    /// Rich, typed error details (`google.rpc.Status.details`).
    pub details: Vec<Any>,
    /// Any additional metadata (e.g. trailing headers) associated with the call.
    pub metadata: Metadata,
}

impl Status {
    /// Create a new status with the given code and message.
    pub fn new(code: Code, message: impl Into<String>) -> Self {
        Status {
            code,
            message: message.into(),
            details: Vec::new(),
            metadata: Metadata::new(),
        }
    }

    /// A successful [`Code::Ok`] status with no message.
    pub fn ok() -> Self {
        Status::new(Code::Ok, "")
    }

    /// Whether this status represents success.
    pub fn is_ok(&self) -> bool {
        self.code == Code::Ok
    }

    /// Attach a rich error detail (`google.protobuf.Any`).
    pub fn with_detail(mut self, detail: Any) -> Self {
        self.details.push(detail);
        self
    }

    /// Recover a status from response trailers.
    pub fn from_trailers(trailers: &Metadata) -> Status {
        let code = trailers
            .get_text("grpc-status")
            .and_then(|s| s.parse::<i32>().ok())
            .map(Code::from_i32)
            .unwrap_or(Code::Unknown);
        let message = trailers.get_text("grpc-message").unwrap_or("").to_string();
        let details = trailers
            .get_binary("grpc-status-details-bin")
            .and_then(|b| decode_google_rpc_status(b).ok())
            .map(|(_, _, d)| d)
            .unwrap_or_default();
        Status {
            code,
            message,
            details,
            metadata: trailers.clone(),
        }
    }

    /// Serialize this status into trailers (`grpc-status`, `grpc-message`,
    /// `grpc-status-details-bin`).
    pub fn to_trailers(&self) -> Metadata {
        let mut m = Metadata::new();
        m.insert_raw("grpc-status", self.code.as_i32().to_string().as_bytes());
        if !self.message.is_empty() {
            m.insert_raw("grpc-message", self.message.as_bytes());
        }
        if !self.details.is_empty() {
            let bytes = encode_google_rpc_status(self.code, &self.message, &self.details);
            m.insert_raw("grpc-status-details-bin", &bytes);
        }
        for key in self.metadata.keys() {
            if !key.starts_with("grpc-") {
                if let Some(v) = self.metadata.get_binary(key) {
                    m.insert_raw(key, v);
                }
            }
        }
        m
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "status {}: {}", self.code, self.message)
    }
}

impl std::error::Error for Status {}

/// Encode a `google.rpc.Status` message.
///
/// Field 1 = code (int32), field 2 = message (string), field 3 = repeated
/// details (`google.protobuf.Any`).
pub(crate) fn encode_google_rpc_status(code: Code, message: &str, details: &[Any]) -> Vec<u8> {
    let mut w = Writer::new();
    scalar::encode_int32(&mut w, 1, code.as_i32());
    if !message.is_empty() {
        scalar::encode_string(&mut w, 2, message);
    }
    for d in details {
        let dbytes = d.encode_to_vec().expect("any encoding must succeed");
        w.write_tag(3, WireType::LengthDelimited);
        w.write_length_delimited(&dbytes);
    }
    w.into_vec()
}

/// Decode a `google.rpc.Status` message.
pub(crate) fn decode_google_rpc_status(bytes: &[u8]) -> Result<(Code, String, Vec<Any>)> {
    let mut r = Reader::new(bytes);
    let mut code = Code::Ok;
    let mut message = String::new();
    let mut details = Vec::new();
    while !r.is_empty() {
        let tag = r.read_tag()?;
        match (tag.field_number, tag.wire_type) {
            (1, _) => code = Code::from_i32(scalar::read_int32(&mut r)?),
            (2, _) => message = r.read_string_owned()?,
            (3, _) => {
                let b = r.read_length_delimited()?;
                let mut ar = Reader::new(b);
                let mut any = Any {
                    type_url: String::new(),
                    value: Vec::new(),
                };
                any.merge_from(&mut ar)?;
                details.push(any);
            }
            _ => r.skip(tag.wire_type)?,
        }
    }
    Ok((code, message, details))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_roundtrip() {
        for i in 0..=16i32 {
            assert_eq!(Code::from_i32(i).as_i32(), i);
        }
        assert_eq!(Code::from_i32(99).as_i32(), 2);
    }

    #[test]
    fn trailers_roundtrip_ok() {
        let s = Status::new(Code::NotFound, "missing");
        let t = s.to_trailers();
        assert_eq!(t.get_text("grpc-status"), Some("5"));
        assert_eq!(t.get_text("grpc-message"), Some("missing"));
        let back = Status::from_trailers(&t);
        assert_eq!(back.code, Code::NotFound);
        assert_eq!(back.message, "missing");
    }

    #[test]
    fn rich_details_roundtrip() {
        let detail = Any {
            type_url: "type.googleapis.com/google.rpc.BadRequest".into(),
            value: vec![0x08, 0x01],
        };
        let s = Status::new(Code::InvalidArgument, "bad").with_detail(detail);
        let t = s.to_trailers();
        assert!(t.contains("grpc-status-details-bin"));
        let back = Status::from_trailers(&t);
        assert_eq!(back.code, Code::InvalidArgument);
        assert_eq!(back.details.len(), 1);
        assert_eq!(back.details[0].type_url, "type.googleapis.com/google.rpc.BadRequest");
    }

    #[test]
    fn missing_status_defaults_unknown() {
        let m = Metadata::new();
        let s = Status::from_trailers(&m);
        assert_eq!(s.code, Code::Unknown);
    }
}
