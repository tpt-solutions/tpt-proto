//! Conformance wire protocol: field numbers, enums, and framing (§4.10, §19).
//!
//! The official `conformance_test_runner` drives a testee over stdin/stdout:
//! each exchange is a 4-byte little-endian length prefix followed by a binary
//! `ConformanceRequest`, and the testee replies with the same framing carrying
//! a `ConformanceResponse`. Field numbers here match the upstream schema so the
//! two interoperate.

use std::io::{self, Read, Write};

// --- ConformanceRequest fields ------------------------------------------------
/// `test_category` (field 1).
pub const REQ_TEST_CATEGORY: i32 = 1;
/// `message_type` (field 2).
pub const REQ_MESSAGE_TYPE: i32 = 2;
/// `protobuf_payload` (oneof, field 3).
pub const REQ_PROTOBUF_PAYLOAD: i32 = 3;
/// `json_payload` (oneof, field 4).
pub const REQ_JSON_PAYLOAD: i32 = 4;
/// `requested_output_format` (field 5).
pub const REQ_REQUESTED_FORMAT: i32 = 5;
/// `text_payload` (oneof, field 7).
pub const REQ_TEXT_PAYLOAD: i32 = 7;
/// `jspb_payload` (oneof, field 8).
pub const REQ_JSPB_PAYLOAD: i32 = 8;

// --- ConformanceResponse result oneof ----------------------------------------
/// `parse_error` (field 1).
pub const RES_PARSE_ERROR: i32 = 1;
/// `serialize_error` (field 2).
pub const RES_SERIALIZE_ERROR: i32 = 2;
/// `runtime_error` (field 3).
pub const RES_RUNTIME_ERROR: i32 = 3;
/// `protobuf_payload` (field 4).
pub const RES_PROTOBUF_PAYLOAD: i32 = 4;
/// `json_payload` (field 5).
pub const RES_JSON_PAYLOAD: i32 = 5;
/// `skipped` (field 6).
pub const RES_SKIPPED: i32 = 6;
/// `text_payload` (field 7).
pub const RES_TEXT_PAYLOAD: i32 = 7;
/// `timeout_error` (field 8).
pub const RES_TIMEOUT_ERROR: i32 = 8;

// --- WireFormat enum values ---------------------------------------------------
/// `WireFormat.UNSPECIFIED`.
pub const FMT_UNSPECIFIED: i32 = 0;
/// `WireFormat.PROTOBUF`.
pub const FMT_PROTOBUF: i32 = 1;
/// `WireFormat.JSON`.
pub const FMT_JSON: i32 = 2;

// --- TestCategory enum values ------------------------------------------------
/// `TestCategory.BINARY_TEST`.
pub const CAT_BINARY: i32 = 1;
/// `TestCategory.JSON_TEST`.
pub const CAT_JSON: i32 = 2;
/// `TestCategory.JSON_IGNORE_UNKNOWN_PARSING_TEST`.
pub const CAT_JSON_IGNORE_UNKNOWN: i32 = 3;

/// Read a single length-prefixed frame. Returns `None` on clean EOF at the
/// start of a frame (i.e. the runner closed the pipe).
pub fn read_frame<R: Read>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut len_buf = [0u8; 4];
    let mut total = 0;
    loop {
        let n = reader.read(&mut len_buf[total..])?;
        if n == 0 {
            if total == 0 {
                return Ok(None);
            }
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "truncated frame length",
            ));
        }
        total += n;
        if total == 4 {
            break;
        }
    }
    let len = u32::from_le_bytes(len_buf) as usize;
    if len > 64 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame too large",
        ));
    }
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    Ok(Some(buf))
}

/// Write a single length-prefixed frame.
pub fn write_frame<W: Write>(writer: &mut W, data: &[u8]) -> io::Result<()> {
    let len = u32::try_from(data.len()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidData, "frame exceeds 4 GiB")
    })?;
    writer.write_all(&len.to_le_bytes())?;
    writer.write_all(data)?;
    writer.flush()?;
    Ok(())
}
