//! Integration tests for the conformance suite (§4.10, §19).
//!
//! These run the built-in harness end-to-end (exercising the full tpt-proto
//! stack: language → compiler → descriptors → reflection → core/wire → json)
//! and verify the framed testee protocol loop against hand-crafted frames.

use std::io::Cursor;

use tpt_proto_conformance::protocol::{read_frame, write_frame, FMT_PROTOBUF, REQ_MESSAGE_TYPE, REQ_PROTOBUF_PAYLOAD, REQ_REQUESTED_FORMAT};
use tpt_proto_conformance::schema::Registry;
use tpt_proto_conformance::testee::run_testee_loop;
use tpt_proto_conformance::{run_all, Status};
use tpt_proto_core::Reader;
use tpt_proto_reflect::{DynamicMessage, ScalarValue, Value};

/// The built-in harness must pass every generated case with zero failures.
#[test]
fn harness_all_cases_pass() {
    let registry = Registry::build();
    let report = run_all(&registry);
    assert_eq!(
        report.failures(),
        0,
        "conformance harness reported failures:\n{}",
        report.render()
    );
    assert!(
        report.total() > 0,
        "conformance harness generated no cases"
    );
}

/// Every required conformance area must be represented in the case set:
/// proto2/proto3/editions, binary/JSON, failures, unknown fields, WKTs.
#[test]
fn harness_covers_required_areas() {
    let registry = Registry::build();
    let report = run_all(&registry);
    let names: Vec<&str> = report.results().iter().map(|r| r.name.as_str()).collect();

    let must_contain = [
        "proto2.TestAllTypes: binary roundtrip",
        "proto3.TestAllTypes: binary roundtrip",
        "editions.TestAllTypes: binary roundtrip",
        "proto2.TestAllTypes: json roundtrip",
        "proto3.TestAllTypes: json roundtrip",
        "editions.TestAllTypes: json roundtrip",
        "proto3: malformed binary is a parse error",
        "proto3: malformed json is a parse error",
        "proto3: unknown field preserved in binary roundtrip",
        "proto3: unknown json field rejected",
        "proto3: unknown json field ignored when requested",
        "wkt: Timestamp JSON is RFC3339",
    ];
    for required in must_contain {
        assert!(
            names.iter().any(|n| n == &required),
            "missing required conformance case: {required}"
        );
    }
}

/// Build a length-framed `ConformanceRequest` asking to parse a small
/// protobuf payload of `proto3.TestAllTypes` and echo it back as protobuf.
fn build_framed_request(registry: &Registry) -> Vec<u8> {
    let pool = &registry.pool;
    let desc = registry.lookup("proto3.TestAllTypes").expect("proto3 TestAllTypes");

    let mut msg = DynamicMessage::new(desc.clone(), pool.clone());
    msg.set_field(1, Value::Scalar(ScalarValue::I64(7)));
    let bin = msg.encode().expect("encode sample");

    let mut req = DynamicMessage::new(registry.request_desc.clone(), pool.clone());
    req.set_field(
        REQ_MESSAGE_TYPE,
        Value::Scalar(ScalarValue::String("proto3.TestAllTypes".into())),
    );
    req.set_field(REQ_REQUESTED_FORMAT, Value::Enum(FMT_PROTOBUF));
    req.set_field(
        REQ_PROTOBUF_PAYLOAD,
        Value::Scalar(ScalarValue::Bytes(bin)),
    );

    let encoded = req.encode().expect("encode request");
    let mut framed = Vec::new();
    write_frame(&mut framed, &encoded).expect("frame request");
    framed
}

/// The testee loop must speak the standard framed protocol: a length-prefixed
/// `ConformanceRequest` in, a length-prefixed `ConformanceResponse` out.
#[test]
fn testee_loop_speaks_framed_protocol() {
    let registry = Registry::build();
    let request_bytes = build_framed_request(&registry);

    let mut out = Vec::new();
    run_testee_loop(&mut Cursor::new(request_bytes), &mut out, &registry)
        .expect("testee loop should not error on a valid frame");

    let mut cursor = Cursor::new(out);
    let frame = read_frame(&mut cursor).expect("response frame").expect("response present");
    assert!(!frame.is_empty(), "response frame was empty");

    // The response must decode as a valid ConformanceResponse carrying a
    // protobuf payload (field 4).
    let resp = DynamicMessage::decode(&registry.pool, registry.response_desc.clone(), &mut Reader::new(&frame))
        .expect("response decodes as ConformanceResponse");
    assert!(
        resp.get_field(4).is_some(),
        "expected protobuf_payload (field 4) in response"
    );
}

/// `Status` must round-trip through the report rendering for CI failure output.
#[test]
fn status_classification_is_stable() {
    let registry = Registry::build();
    let report = run_all(&registry);
    let pass = report.results().iter().filter(|r| r.status == Status::Pass).count();
    assert_eq!(pass, report.passed());
}
