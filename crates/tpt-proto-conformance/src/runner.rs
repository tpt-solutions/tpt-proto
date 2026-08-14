//! Conformance harness: generates test cases across all required conformance
//! areas (proto2/proto3/editions, binary/JSON, failures, unknown fields, WKT)
//! and drives them through [`crate::testee::process`], producing a report
//! (§4.10, §19).

use std::sync::Arc;

use tpt_proto_core::Reader;
use tpt_proto_json::{message_to_json_string, JsonOptions};
use tpt_proto_reflect::{DynamicMessage, ScalarValue, Value};

use crate::protocol::*;
use crate::schema::Registry;
use crate::testee::{bytes_value, enum_value, str_value};
use tpt_proto_descriptor::DescriptorProto;

/// Outcome of an individual test case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// Behavior matched expectations.
    Pass,
    /// Behavior diverged from expectations.
    Fail,
    /// The testee legitimately could not run the case (unsupported).
    Skip,
}

/// The result of running one case.
#[derive(Debug, Clone)]
pub struct CaseResult {
    /// Human-readable case name.
    pub name: String,
    /// Outcome.
    pub status: Status,
    /// Detail on failure (empty when passing).
    pub detail: String,
}

/// The expected behavior for a case.
enum Expected {
    /// Output binary must decode to exactly `msg`.
    BinaryEquals(Arc<tpt_proto_descriptor::DescriptorProto>, DynamicMessage),
    /// Output JSON must equal the canonical serialization of `msg`.
    JsonEquals(Arc<tpt_proto_descriptor::DescriptorProto>, DynamicMessage),
    /// The testee must return `parse_error`.
    ParseError,
    /// Output binary must preserve an unknown field with the given number.
    HasUnknown(u32),
    /// The testee must succeed without an error response (used for the
    /// ignore-unknown-fields JSON category).
    Succeed,
}

/// A single conformance case.
struct Case {
    name: String,
    request: DynamicMessage,
    expected: Expected,
}

/// Aggregate result of a conformance run.
#[derive(Debug, Default, Clone)]
pub struct Report {
    results: Vec<CaseResult>,
}

impl Report {
    /// Number of passing cases.
    pub fn passed(&self) -> usize {
        self.results.iter().filter(|r| r.status == Status::Pass).count()
    }

    /// Number of failing cases (excludes intentional skips).
    pub fn failures(&self) -> usize {
        self.results.iter().filter(|r| r.status == Status::Fail).count()
    }

    /// Number of skipped cases.
    pub fn skipped(&self) -> usize {
        self.results.iter().filter(|r| r.status == Status::Skip).count()
    }

    /// Total number of cases.
    pub fn total(&self) -> usize {
        self.results.len()
    }

    /// Append one case result.
    fn push(&mut self, r: CaseResult) {
        self.results.push(r);
    }

    /// All per-case results.
    pub fn results(&self) -> &[CaseResult] {
        &self.results
    }

    /// Render a human-readable failure report.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "conformance: {} passed, {} failed, {} skipped ({} total)\n",
            self.passed(),
            self.failures(),
            self.skipped(),
            self.total()
        ));
        for r in &self.results {
            if r.status == Status::Fail {
                out.push_str(&format!("  FAIL  {}\n        {}\n", r.name, r.detail));
            }
        }
        out
    }

    /// Render a machine-readable (JSON) report suitable for CI artifact upload.
    pub fn to_json(&self) -> String {
        use serde_json::{json, Value as J};
        let results: Vec<J> = self
            .results
            .iter()
            .map(|r| {
                json!({
                    "name": r.name,
                    "status": match r.status {
                        Status::Pass => "PASS",
                        Status::Fail => "FAIL",
                        Status::Skip => "SKIP",
                    },
                    "detail": r.detail,
                })
            })
            .collect();
        let v = json!({
            "total": self.total(),
            "passed": self.passed(),
            "failed": self.failures(),
            "skipped": self.skipped(),
            "results": results,
        });
        serde_json::to_string_pretty(&v).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }
}

/// Build a request message for the given message type and payload.
fn make_request(
    registry: &Registry,
    message_type: &str,
    payload_field: i32,
    payload: Value,
    requested: i32,
) -> DynamicMessage {
    let mut req = DynamicMessage::new(registry.request_desc.clone(), registry.pool.clone());
    req.set_field(REQ_MESSAGE_TYPE, str_value(message_type));
    req.set_field(REQ_REQUESTED_FORMAT, enum_value(requested));
    req.set_field(payload_field, payload);
    req
}

/// Look up a top-level `NestedMessage`/`NestedEnum` descriptor defined per
/// dialect package (`proto2`/`proto3`/`editions`).
fn lookup_nested(registry: &Registry, name: &str) -> Arc<DescriptorProto> {
    for pkg in ["proto3", "proto2", "editions"] {
        if let Some(d) = registry.lookup(&format!("{pkg}.{name}")) {
            return d;
        }
    }
    panic!("nested descriptor {name} not found");
}

/// Construct a representative `TestAllTypes` message for the given dialect's
/// descriptor.
fn build_sample(registry: &Registry, desc: &Arc<DescriptorProto>) -> DynamicMessage {
    let pool = &registry.pool;
    let mut msg = DynamicMessage::new(desc.clone(), pool.clone());

    msg.set_field(1, Value::Scalar(ScalarValue::I64(42)));
    msg.set_field(2, Value::Scalar(ScalarValue::I64(-7)));
    msg.set_field(3, Value::Scalar(ScalarValue::U64(9)));
    msg.set_field(4, Value::Scalar(ScalarValue::U64(10)));
    msg.set_field(5, Value::Scalar(ScalarValue::I64(-11)));
    msg.set_field(6, Value::Scalar(ScalarValue::I64(-12)));
    msg.set_field(7, Value::Scalar(ScalarValue::U64(13)));
    msg.set_field(8, Value::Scalar(ScalarValue::U64(14)));
    msg.set_field(9, Value::Scalar(ScalarValue::I64(-15)));
    msg.set_field(10, Value::Scalar(ScalarValue::I64(-16)));
    msg.set_field(11, Value::Scalar(ScalarValue::F64(1.5)));
    msg.set_field(12, Value::Scalar(ScalarValue::F64(-2.5)));
    msg.set_field(13, Value::Scalar(ScalarValue::Bool(true)));
    msg.set_field(14, Value::Scalar(ScalarValue::String("hello".into())));
    msg.set_field(15, Value::Scalar(ScalarValue::Bytes(b"bytes".to_vec())));

    let nested = lookup_nested(registry, "NestedMessage");
    let mut nm = DynamicMessage::new(nested.clone(), pool.clone());
    nm.set_field(1, Value::Scalar(ScalarValue::I64(100)));
    nm.set_field(2, Value::Scalar(ScalarValue::String("nested".into())));
    msg.set_field(18, Value::Message(nm));
    msg.set_field(21, Value::Enum(1)); // BAR

    msg.set_field(
        31,
        Value::List(vec![
            Value::Scalar(ScalarValue::I64(1)),
            Value::Scalar(ScalarValue::I64(2)),
            Value::Scalar(ScalarValue::I64(3)),
        ]),
    );
    msg.set_field(
        34,
        Value::List(vec![
            Value::Scalar(ScalarValue::String("a".into())),
            Value::Scalar(ScalarValue::String("b".into())),
        ]),
    );

    msg.set_field(
        56,
        Value::Map(vec![
            (Value::Scalar(ScalarValue::I64(1)), Value::Scalar(ScalarValue::I64(10))),
            (Value::Scalar(ScalarValue::I64(2)), Value::Scalar(ScalarValue::I64(20))),
        ]),
    );
    msg.set_field(
        58,
        Value::Map(vec![(
            Value::Scalar(ScalarValue::String("k".into())),
            Value::Scalar(ScalarValue::String("v".into())),
        )]),
    );

    msg.set_field(113, Value::Scalar(ScalarValue::String("oneof".into())));

    // well-known type: Timestamp.
    if let Some(ts_desc) = registry.lookup("google.protobuf.Timestamp") {
        let mut ts = DynamicMessage::new(ts_desc.clone(), pool.clone());
        ts.set_field(1, Value::Scalar(ScalarValue::I64(61)));
        ts.set_field(2, Value::Scalar(ScalarValue::I64(210_000_000)));
        msg.set_field(80, Value::Message(ts));
    }

    msg
}

/// Build the per-dialect round-trip cases for `message_type`.
fn dialect_cases(registry: &Registry, message_type: &str) -> Vec<Case> {
    let desc = match registry.lookup(message_type) {
        Some(d) => d,
        None => return Vec::new(),
    };
    let sample = build_sample(registry, &desc);
    let json = message_to_json_string(&registry.pool, &desc, &sample, &JsonOptions::default())
        .expect("sample -> json");

    let mut cases = Vec::new();

    // binary -> binary roundtrip.
    let bin = sample.encode().expect("sample -> binary");
    cases.push(Case {
        name: format!("{message_type}: binary roundtrip"),
        request: make_request(
            registry,
            message_type,
            REQ_PROTOBUF_PAYLOAD,
            bytes_value(bin.clone()),
            FMT_PROTOBUF,
        ),
        expected: Expected::BinaryEquals(desc.clone(), sample.clone()),
    });

    // json -> json roundtrip.
    cases.push(Case {
        name: format!("{message_type}: json roundtrip"),
        request: make_request(
            registry,
            message_type,
            REQ_JSON_PAYLOAD,
            str_value(&json),
            FMT_JSON,
        ),
        expected: Expected::JsonEquals(desc.clone(), sample.clone()),
    });

    // json -> binary.
    cases.push(Case {
        name: format!("{message_type}: json to binary"),
        request: make_request(
            registry,
            message_type,
            REQ_JSON_PAYLOAD,
            str_value(&json),
            FMT_PROTOBUF,
        ),
        expected: Expected::BinaryEquals(desc.clone(), sample.clone()),
    });

    // binary -> json.
    cases.push(Case {
        name: format!("{message_type}: binary to json"),
        request: make_request(
            registry,
            message_type,
            REQ_PROTOBUF_PAYLOAD,
            bytes_value(bin),
            FMT_JSON,
        ),
        expected: Expected::JsonEquals(desc, sample),
    });

    cases
}

/// Build the cross-cutting (failure / unknown / wkt) cases.
fn behavior_cases(registry: &Registry) -> Vec<Case> {
    let mut cases = Vec::new();
    let p3 = "proto3.TestAllTypes";

    // Failure behavior: malformed binary -> parse_error.
    cases.push(Case {
        name: "proto3: malformed binary is a parse error".into(),
        request: make_request(
            registry,
            p3,
            REQ_PROTOBUF_PAYLOAD,
            bytes_value(vec![0x0A, 0x05, 0x01]),
            FMT_PROTOBUF,
        ),
        expected: Expected::ParseError,
    });

    // Failure behavior: malformed JSON -> parse_error.
    cases.push(Case {
        name: "proto3: malformed json is a parse error".into(),
        request: make_request(
            registry,
            p3,
            REQ_JSON_PAYLOAD,
            str_value("{ this is not json"),
            FMT_JSON,
        ),
        expected: Expected::ParseError,
    });

    // Unknown field handling (binary): unknown field preserved on roundtrip.
    let desc = registry.lookup(p3).expect("proto3 TestAllTypes");
    let sample = build_sample(registry, &desc);
    let mut base = sample.encode().expect("sample encode");
    // Append an unknown field 9000 (varint 42).
    let mut extra = tpt_proto_core::Writer::new();
    extra.write_tag(9000, tpt_proto_core::WireType::Varint);
    extra.write_varint(42);
    base.extend_from_slice(extra.buf());
    cases.push(Case {
        name: "proto3: unknown field preserved in binary roundtrip".into(),
        request: make_request(
            registry,
            p3,
            REQ_PROTOBUF_PAYLOAD,
            bytes_value(base),
            FMT_PROTOBUF,
        ),
        expected: Expected::HasUnknown(9000),
    });

    // Unknown field handling (json): unknown field rejected by default.
    cases.push(Case {
        name: "proto3: unknown json field rejected".into(),
        request: make_request(
            registry,
            p3,
            REQ_JSON_PAYLOAD,
            str_value(r#"{"unknown_field": 1}"#),
            FMT_JSON,
        ),
        expected: Expected::ParseError,
    });

    // Unknown field handling (json): ignored under ignore-unknown category.
    cases.push(Case {
        name: "proto3: unknown json field ignored when requested".into(),
        request: {
            let mut req = make_request(
                registry,
                p3,
                REQ_JSON_PAYLOAD,
                str_value(r#"{"unknown_field": 1}"#),
                FMT_PROTOBUF,
            );
            req.set_field(REQ_TEST_CATEGORY, enum_value(CAT_JSON_IGNORE_UNKNOWN));
            req
        },
        expected: Expected::Succeed,
    });

    // Well-known type behavior: Timestamp emits RFC3339 in JSON.
    let ts_desc = registry.lookup("google.protobuf.Timestamp").expect("Timestamp");
    let mut ts = DynamicMessage::new(ts_desc.clone(), registry.pool.clone());
    ts.set_field(1, Value::Scalar(ScalarValue::I64(61)));
    ts.set_field(2, Value::Scalar(ScalarValue::I64(210_000_000)));
    let ts_json = message_to_json_string(&registry.pool, &ts_desc, &ts, &JsonOptions::default())
        .expect("ts -> json");
    cases.push(Case {
        name: "wkt: Timestamp JSON is RFC3339".into(),
        request: make_request(
            registry,
            "google.protobuf.Timestamp",
            REQ_JSON_PAYLOAD,
            str_value(&ts_json),
            FMT_JSON,
        ),
        expected: Expected::JsonEquals(ts_desc, ts),
    });

    cases
}

/// Generate the full conformance case set.
fn generate_cases(registry: &Registry) -> Vec<Case> {
    let mut cases = Vec::new();
    for mt in [
        "proto2.TestAllTypes",
        "proto3.TestAllTypes",
        "editions.TestAllTypes",
    ] {
        cases.extend(dialect_cases(registry, mt));
    }
    cases.extend(behavior_cases(registry));
    cases
}

/// If the response is an error/skip oneof member, return a human description
/// (or `None` if it carries a real payload).
fn error_detail(response: &DynamicMessage) -> Option<String> {
    if let Some(Value::Scalar(ScalarValue::String(s))) = response.get_field(RES_PARSE_ERROR) {
        return Some(format!("parse_error: {s}"));
    }
    if let Some(Value::Scalar(ScalarValue::String(s))) = response.get_field(RES_SERIALIZE_ERROR) {
        return Some(format!("serialize_error: {s}"));
    }
    if let Some(Value::Scalar(ScalarValue::String(s))) = response.get_field(RES_RUNTIME_ERROR) {
        return Some(format!("runtime_error: {s}"));
    }
    if response.get_field(RES_SKIPPED).is_some() {
        return Some("skipped".into());
    }
    None
}

/// Run a single case and report its result.
fn run_case(registry: &Registry, case: &Case) -> CaseResult {
    let response = crate::testee::process(registry, &case.request);
    match &case.expected {
        Expected::ParseError => {
            let ok = response.get_field(RES_PARSE_ERROR).is_some();
            CaseResult {
                name: case.name.clone(),
                status: if ok { Status::Pass } else { Status::Fail },
                detail: if ok {
                    String::new()
                } else {
                    "expected parse_error".into()
                },
            }
        }
        Expected::Succeed => {
            let ok = error_detail(&response).is_none();
            CaseResult {
                name: case.name.clone(),
                status: if ok { Status::Pass } else { Status::Fail },
                detail: if ok {
                    String::new()
                } else {
                    format!("expected success, got: {}", error_detail(&response).unwrap())
                },
            }
        }
        Expected::HasUnknown(field) => {
            if let Some(detail) = error_detail(&response) {
                return CaseResult {
                    name: case.name.clone(),
                    status: Status::Fail,
                    detail,
                };
            }
            match response.get_field(RES_PROTOBUF_PAYLOAD) {
                Some(Value::Scalar(ScalarValue::Bytes(b))) => {
                    let dm = DynamicMessage::decode(
                        &registry.pool,
                        registry.lookup("proto3.TestAllTypes").unwrap(),
                        &mut Reader::new(b),
                    );
                    match dm {
                        Ok(m) => {
                            let found = m.unknown.iter().any(|(f, v)| {
                                f == *field
                                    && matches!(v, tpt_proto_core::UnknownValue::Varint(42))
                            });
                            CaseResult {
                                name: case.name.clone(),
                                status: if found { Status::Pass } else { Status::Fail },
                                detail: if found {
                                    String::new()
                                } else {
                                    "unknown field 9000 not preserved".into()
                                },
                            }
                        }
                        Err(e) => CaseResult {
                            name: case.name.clone(),
                            status: Status::Fail,
                            detail: format!("output failed to decode: {e}"),
                        },
                    }
                }
                _ => CaseResult {
                    name: case.name.clone(),
                    status: Status::Fail,
                    detail: "expected protobuf_payload".into(),
                },
            }
        }
        Expected::BinaryEquals(desc, expected) => {
            if let Some(detail) = error_detail(&response) {
                return CaseResult {
                    name: case.name.clone(),
                    status: Status::Fail,
                    detail,
                };
            }
            match response.get_field(RES_PROTOBUF_PAYLOAD) {
                Some(Value::Scalar(ScalarValue::Bytes(b))) => {
                    match DynamicMessage::decode(
                        &registry.pool,
                        desc.clone(),
                        &mut Reader::new(b),
                    ) {
                        Ok(m) => {
                            let got = message_to_json_string(
                                &registry.pool,
                                desc,
                                &m,
                                &JsonOptions::default(),
                            )
                            .unwrap_or_default();
                            let want = message_to_json_string(
                                &registry.pool,
                                desc,
                                expected,
                                &JsonOptions::default(),
                            )
                            .unwrap_or_default();
                            let ok = got == want;
                            CaseResult {
                                name: case.name.clone(),
                                status: if ok { Status::Pass } else { Status::Fail },
                                detail: if ok {
                                    String::new()
                                } else {
                                    format!("binary roundtrip mismatch:\n  got:  {got}\n  want: {want}")
                                },
                            }
                        }
                        Err(e) => CaseResult {
                            name: case.name.clone(),
                            status: Status::Fail,
                            detail: format!("output failed to decode: {e}"),
                        },
                    }
                }
                _ => CaseResult {
                    name: case.name.clone(),
                    status: Status::Fail,
                    detail: "expected protobuf_payload".into(),
                },
            }
        }
        Expected::JsonEquals(desc, expected) => {
            if let Some(detail) = error_detail(&response) {
                return CaseResult {
                    name: case.name.clone(),
                    status: Status::Fail,
                    detail,
                };
            }
            match response.get_field(RES_JSON_PAYLOAD) {
                Some(Value::Scalar(ScalarValue::String(s))) => {
                    let want = message_to_json_string(
                        &registry.pool,
                        desc,
                        expected,
                        &JsonOptions::default(),
                    )
                    .expect("expected -> json");
                    let ok = s.trim() == want.trim();
                    CaseResult {
                        name: case.name.clone(),
                        status: if ok { Status::Pass } else { Status::Fail },
                        detail: if ok {
                            String::new()
                        } else {
                            format!("json mismatch:\n  got:  {s}\n  want: {want}")
                        },
                    }
                }
                _ => CaseResult {
                    name: case.name.clone(),
                    status: Status::Fail,
                    detail: "expected json_payload".into(),
                },
            }
        }
    }
}

/// Run every generated case and return the aggregate report.
pub fn run_all(registry: &Registry) -> Report {
    let cases = generate_cases(registry);
    let mut report = Report::default();
    for case in &cases {
        report.push(run_case(registry, case));
    }
    report
}
