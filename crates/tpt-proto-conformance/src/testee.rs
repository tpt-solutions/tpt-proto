//! Conformance testee: process one `ConformanceRequest` into a response and
//! drive the framed stdin/stdout loop (§4.10, §19).

use std::io::{self, Read, Write};

use tpt_proto_core::Reader;
use tpt_proto_json::{message_to_json_string, json_string_to_message, JsonOptions};
use tpt_proto_reflect::{DynamicMessage, ScalarValue, Value};

use crate::protocol::*;
use crate::schema::Registry;

/// A string-valued field (for request/response construction).
pub(crate) fn str_value(s: &str) -> Value {
    Value::Scalar(Scalar::String(s.to_string()))
}

/// A bytes-valued field.
pub(crate) fn bytes_value(b: Vec<u8>) -> Value {
    Value::Scalar(Scalar::Bytes(b))
}

/// An enum-valued field (categories / wire formats).
pub(crate) fn enum_value(n: i32) -> Value {
    Value::Enum(n)
}

/// A bool-valued field.
pub(crate) fn bool_value(b: bool) -> Value {
    Value::Scalar(Scalar::Bool(b))
}

fn get_str(m: &DynamicMessage, f: i32) -> Option<String> {
    match m.get_field(f) {
        Some(Value::Scalar(Scalar::String(s))) => Some(s.clone()),
        _ => None,
    }
}

fn get_bytes(m: &DynamicMessage, f: i32) -> Option<Vec<u8>> {
    match m.get_field(f) {
        Some(Value::Scalar(Scalar::Bytes(b))) => Some(b.clone()),
        _ => None,
    }
}

fn get_enum(m: &DynamicMessage, f: i32) -> Option<i32> {
    match m.get_field(f) {
        Some(Value::Enum(n)) => Some(*n),
        _ => None,
    }
}

/// Process a single conformance request into a response.
///
/// Semantics mirror the official testee contract:
/// * unknown `message_type` or unsupported input/output formats → `skipped`,
/// * parse failure → `parse_error`,
/// * serialize failure → `serialize_error`,
/// * success → `protobuf_payload` / `json_payload`.
pub fn process(registry: &Registry, request: &DynamicMessage) -> DynamicMessage {
    let mut response = DynamicMessage::new(registry.response_desc.clone(), registry.pool.clone());

    let message_type = get_str(request, REQ_MESSAGE_TYPE).unwrap_or_default();
    let requested = get_enum(request, REQ_REQUESTED_FORMAT).unwrap_or(FMT_UNSPECIFIED);
    let category = get_enum(request, REQ_TEST_CATEGORY).unwrap_or(0);

    // Unsupported input formats: text / jspb payloads.
    if request.get_field(REQ_TEXT_PAYLOAD).is_some()
        || request.get_field(REQ_JSPB_PAYLOAD).is_some()
    {
        response.set_field(RES_SKIPPED, bool_value(true));
        return response;
    }

    let Some(desc) = registry.lookup(&message_type) else {
        response.set_field(RES_SKIPPED, bool_value(true));
        return response;
    };

    // Parse the input payload into a dynamic message.
    let msg = if let Some(bytes) = get_bytes(request, REQ_PROTOBUF_PAYLOAD) {
        match DynamicMessage::decode(&registry.pool, desc.clone(), &mut Reader::new(&bytes)) {
            Ok(m) => m,
            Err(e) => {
                response.set_field(RES_PARSE_ERROR, str_value(&format!("failed to parse binary: {e}")));
                return response;
            }
        }
    } else if let Some(json) = get_str(request, REQ_JSON_PAYLOAD) {
        let mut opts = JsonOptions::default();
        if category == CAT_JSON_IGNORE_UNKNOWN {
            opts.ignore_unknown_fields = true;
        }
        match json_string_to_message(&registry.pool, &desc, &json, &opts) {
            Ok(m) => m,
            Err(e) => {
                response.set_field(
                    RES_PARSE_ERROR,
                    str_value(&format!("failed to parse JSON: {e}")),
                );
                return response;
            }
        }
    } else {
        response.set_field(
            RES_RUNTIME_ERROR,
            str_value("request contained no payload"),
        );
        return response;
    };

    // Serialize to the requested output format.
    match requested {
        FMT_PROTOBUF => match msg.encode() {
            Ok(b) => response.set_field(RES_PROTOBUF_PAYLOAD, bytes_value(b)),
            Err(e) => response.set_field(
                RES_SERIALIZE_ERROR,
                str_value(&format!("failed to serialize binary: {e}")),
            ),
        },
        FMT_JSON => {
            match message_to_json_string(&registry.pool, &desc, &msg, &JsonOptions::default()) {
                Ok(s) => response.set_field(RES_JSON_PAYLOAD, str_value(&s)),
                Err(e) => response.set_field(
                    RES_SERIALIZE_ERROR,
                    str_value(&format!("failed to serialize JSON: {e}")),
                ),
            }
        }
        _ => {
            // Unsupported output format (JSPB / unspecified).
            response.set_field(RES_SKIPPED, bool_value(true));
        }
    }

    response
}

/// Run the conformance testee loop over the given reader/writer until EOF.
pub fn run_testee_loop<R: Read, W: Write>(
    reader: &mut R,
    writer: &mut W,
    registry: &Registry,
) -> io::Result<()> {
    while let Some(frame) = read_frame(reader)? {
        let request = match DynamicMessage::decode(
            &registry.pool,
            registry.request_desc.clone(),
            &mut Reader::new(&frame),
        ) {
            Ok(r) => r,
            Err(e) => {
                let mut resp =
                    DynamicMessage::new(registry.response_desc.clone(), registry.pool.clone());
                resp.set_field(
                    RES_RUNTIME_ERROR,
                    str_value(&format!("failed to parse ConformanceRequest: {e}")),
                );
                let out = resp
                    .encode()
                    .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?;
                write_frame(writer, &out)?;
                continue;
            }
        };
        let resp = process(registry, &request);
        let out = resp
            .encode()
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
        write_frame(writer, &out)?;
    }
    Ok(())
}

type Scalar = ScalarValue;
