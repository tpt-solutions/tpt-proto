//! Round-trip integration tests for the generated sample types.
//!
//! The generated code (commit-friendly copy in `sample_generated.rs`) is
//! included verbatim and exercised through the `tpt_proto_core::Message`
//! contract: encode → decode must be identity for scalars, messages, repeated
//! (packed) fields, maps, oneofs, and enums (including unknown values).

include!("./sample_generated.rs");

#[test]
fn roundtrip_scalars_and_nested() {
    let mut m = SampleScalars::default();
    m.d = 1.5;
    m.f = -0.25;
    m.i32 = -3;
    m.i64 = 1_000_000_000;
    m.u32 = 42;
    m.u64 = 99;
    m.s32 = -7;
    m.s64 = -8;
    m.f32 = 9;
    m.f64 = 10;
    m.sf32 = -11;
    m.sf64 = -12;
    m.b = true;
    m.s = "héllo".into();
    m.by = vec![1, 2, 3];
    m.nested = SampleNested {
        name: "nested".into(),
        ..Default::default()
    };

    let bytes = m.encode_to_vec().unwrap();
    let m2 = SampleScalars::decode(&bytes).unwrap();
    assert_eq!(m, m2);
}

#[test]
fn roundtrip_collections_packed() {
    let mut c = SampleCollections::default();
    c.nums = vec![1, 2, 300, -4];
    c.strs = vec!["a".into(), "b".into()];
    c.items.push(SampleNested {
        name: "item".into(),
        ..Default::default()
    });
    c.color = SampleColor::GREEN;
    c.colors = vec![
        SampleColor::RED,
        SampleColor::BLUE,
        SampleColor::Unknown(99),
    ];

    let bytes = c.encode_to_vec().unwrap();
    let c2 = SampleCollections::decode(&bytes).unwrap();
    assert_eq!(c, c2);
}

#[test]
fn roundtrip_maps() {
    let mut c = SampleCollections::default();
    c.labels.insert("alpha".into(), 1);
    c.labels.insert("beta".into(), 2);
    c.by_id.insert(
        7,
        SampleNested {
            name: "seven".into(),
            ..Default::default()
        },
    );

    let bytes = c.encode_to_vec().unwrap();
    let c2 = SampleCollections::decode(&bytes).unwrap();
    assert_eq!(c, c2);
}

#[test]
fn roundtrip_oneof() {
    // Each oneof case must survive independently.
    for built in [
        SampleWithOneofBuilder::new().a("email".into()).build(),
        SampleWithOneofBuilder::new().n(123).build(),
        SampleWithOneofBuilder::new()
            .obj(SampleNested {
                name: "obj".into(),
                ..Default::default()
            })
            .build(),
    ] {
        let bytes = built.encode_to_vec().unwrap();
        let decoded = SampleWithOneof::decode(&bytes).unwrap();
        assert_eq!(built, decoded);
    }
}

#[test]
fn unknown_fields_preserved() {
    // Build with a field, then append a synthetic unknown field, and confirm
    // the unknown bytes are re-emitted on re-encode.
    let mut m = SampleScalars::default();
    m.s = "kept".into();
    let mut bytes = m.encode_to_vec().unwrap();

    // Append an unknown field 99 (varint 5) to the encoded output.
    use tpt_proto_core::scalar;
    use tpt_proto_core::Writer;
    let mut extra = Writer::new();
    scalar::encode_int32(&mut extra, 99, 5);
    bytes.extend_from_slice(extra.buf());

    let decoded = SampleScalars::decode(&bytes).unwrap();
    assert_eq!(decoded.s, "kept");
    assert_eq!(decoded.unknown_fields.len(), 1);

    // Re-encoding should preserve the unknown field.
    let re = decoded.encode_to_vec().unwrap();
    assert!(re.len() > m.encode_to_vec().unwrap().len());
}

#[test]
fn builder_and_defaults() {
    let m = SampleScalarsBuilder::new()
        .i32(7)
        .s("builder".into())
        .build();
    assert_eq!(m.i32, 7);
    assert_eq!(m.s, "builder");
    assert_eq!(SampleScalars::default().i32, 0);
}

#[test]
fn proto_full_name_hook() {
    assert_eq!(SampleScalars::PROTO_FULL_NAME, "sample.Scalars");
    assert_eq!(SampleCollections::PROTO_FULL_NAME, "sample.Collections");
}

#[test]
fn json_roundtrip_scalars() {
    let mut m = SampleScalars::default();
    m.d = 1.5;
    m.i32 = -3;
    m.u64 = 99;
    m.s = "héllo".into();
    m.by = vec![1, 2, 3];
    m.nested = SampleNested {
        name: "nested".into(),
        ..Default::default()
    };

    let json = m.to_json().expect("to_json failed");
    let back = SampleScalars::from_json(&json).expect("from_json failed");
    assert_eq!(m, back);
}

#[test]
fn json_roundtrip_collections() {
    let mut c = SampleCollections::default();
    c.nums = vec![1, 2, 300, -4];
    c.strs = vec!["a".into(), "b".into()];
    c.color = SampleColor::GREEN;
    c.labels.insert("alpha".into(), 1);
    c.by_id.insert(
        7,
        SampleNested {
            name: "seven".into(),
            ..Default::default()
        },
    );

    let json = c.to_json().expect("to_json failed");
    let back = SampleCollections::from_json(&json).expect("from_json failed");
    assert_eq!(c, back);
}

#[test]
fn json_roundtrip_oneof() {
    let built = SampleWithOneofBuilder::new().n(123).build();
    let json = built.to_json().expect("to_json failed");
    let back = SampleWithOneof::from_json(&json).expect("from_json failed");
    assert_eq!(built, back);
}

#[test]
fn text_roundtrip_scalars() {
    let mut m = SampleScalars::default();
    m.i32 = 7;
    m.s = "text".into();

    let text = m.to_text().expect("to_text failed");
    let back = SampleScalars::from_text(&text).expect("from_text failed");
    assert_eq!(m, back);
}

#[test]
fn text_roundtrip_collections() {
    let mut c = SampleCollections::default();
    c.nums = vec![5, 6];
    c.labels.insert("k".into(), 9);

    let text = c.to_text().expect("to_text failed");
    let back = SampleCollections::from_text(&text).expect("from_text failed");
    assert_eq!(c, back);
}
