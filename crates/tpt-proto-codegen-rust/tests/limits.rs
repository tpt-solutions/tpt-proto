//! Regression tests: `DecoderLimits` enforcement through the **generated
//! struct** decode path.
//!
//! Guards against reintroduction of the audit finding where generated
//! `merge_from` created a fresh `Reader` per nested field, resetting depth to
//! zero and silently discarding caller-supplied limits.

// The checked-in generated sample (same file used by roundtrip.rs).
include!("./sample_generated.rs");

use tpt_proto_core::{decode_with_limits, DecoderLimits};

fn nested_sample() -> Vec<u8> {
    let mut m = SampleScalars::default();
    m.i32 = 7;
    m.nested = SampleNested {
        name: "deep".into(),
        ..Default::default()
    };
    m.encode_to_vec().unwrap()
}

#[test]
fn default_limits_accept_nested_generated() {
    let bytes = nested_sample();
    let m = SampleScalars::decode(&bytes).unwrap();
    assert_eq!(m.nested.name, "deep");
}

/// A tightened `max_depth` passed through `decode_with_limits` must reach the
/// generated nested-field reader: with `max_depth = 0`, opening the child
/// reader for `nested` exceeds depth and the decode must fail even though the
/// payload itself is tiny and perfectly valid under default limits.
#[test]
fn tight_max_depth_rejected_through_generated_path() {
    let bytes = nested_sample();
    let mut tight = DecoderLimits::default();
    tight.max_depth = 0;
    let err = decode_with_limits::<SampleScalars>(&bytes, tight).unwrap_err();
    assert!(
        matches!(err, tpt_proto_core::Error::DepthLimitExceeded),
        "expected DepthLimitExceeded, got {err:?}"
    );

    // The same payload decodes fine one level deeper.
    let mut ok = DecoderLimits::default();
    ok.max_depth = 1;
    decode_with_limits::<SampleScalars>(&bytes, ok).unwrap();
}

/// A tightened `max_bytes` must likewise be honored by the generated path.
#[test]
fn tight_max_bytes_rejected_through_generated_path() {
    let bytes = nested_sample();
    let mut tight = DecoderLimits::default();
    tight.max_bytes = Some(2);
    assert!(decode_with_limits::<SampleScalars>(&bytes, tight).is_err());
}

/// Source-level guard: the generator's nested-message template must descend
/// through the inheriting `Reader::nested(...)` constructor, never a fresh
/// `Reader::new(...)` over the body (which would reset depth and limits at
/// every level, including for self-referential schemas).
#[test]
fn generated_templates_use_inheriting_nested_readers() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs")).unwrap();
    assert!(
        src.contains(".nested(__body)?"),
        "codegen no longer opens nested readers via Reader::nested"
    );
    assert!(
        !src.contains("Reader::new(__body)"),
        "codegen regressed: fresh Reader::new over nested bodies resets depth/limits"
    );
}
