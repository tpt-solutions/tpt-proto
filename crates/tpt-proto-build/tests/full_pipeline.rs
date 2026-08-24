//! Full-pipeline integration test: a real `.proto` file flows through
//! `tpt-proto-build` (build.rs-style compilation with incremental rebuild),
//! the compiler + codegen APIs directly, and then through the runtime,
//! reflection, JSON, text, and CLI-library conversion layers — tying every
//! major component together in one pass.

use std::path::PathBuf;

use tpt_proto_build::{compile_protos, BuildConfig};

const PROTO_SRC: &str = r#"
syntax = "proto3";
package demo;

// A demo user record.
message User {
  int64 id = 1;
  string name = 2;
}
"#;

#[test]
fn full_pipeline_proto_build_codegen_runtime_json_text_cli() {
    let root =
        std::env::temp_dir().join(format!("tpt_fullpipe_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("proto")).unwrap();
    let proto_path = root.join("proto").join("user.proto");
    std::fs::write(&proto_path, PROTO_SRC).unwrap();
    let out_dir: PathBuf = root.join("out");

    // ---- Stage 1: build-crate compilation -------------------------------
    let config = BuildConfig { incremental: true, ..Default::default() };
    compile_protos(
        &[proto_path.clone()],
        &[root.join("proto")],
        &out_dir,
        &config,
    )
    .expect("compile_protos failed");

    let generated = std::fs::read_to_string(out_dir.join("user.rs")).unwrap();
    assert!(
        generated.contains("pub struct DemoUser"),
        "generated code missing DemoUser"
    );
    let mod_rs = std::fs::read_to_string(out_dir.join("mod.rs")).unwrap();
    assert!(mod_rs.contains("pub mod user;"));

    // ---- Stage 2: compiler + codegen APIs see the same schema -----------
    use tpt_proto_compiler::compile;
    use tpt_proto_language::parse_file;
    let parsed = parse_file("user.proto", PROTO_SRC);
    assert!(!parsed.diagnostics.has_errors(), "parse diagnostics");
    let (fd, cdiags) = compile(&parsed.file);
    assert!(!cdiags.has_errors(), "compile diagnostics");

    // ---- Stage 3: runtime / reflection / JSON / text / CLI layers -------
    use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue, Value};
    let pool = DescriptorPool::from_file(&fd);
    let desc = pool.lookup_message("demo.User").unwrap();

    let mut dm = DynamicMessage::new(desc.clone(), pool.clone());
    dm.set_field(1, Value::Scalar(ScalarValue::I64(7)));
    dm.set_field(2, Value::Scalar(ScalarValue::String("ada".into())));
    let bytes = dm.encode().unwrap();

    // JSON via the CLI library helpers.
    let json = tpt_proto_cli::binary_to_json(&pool, &desc, &bytes).unwrap();
    assert!(json.contains("7"), "unexpected json: {json}");
    let rebytes = tpt_proto_cli::json_to_binary(&pool, &desc, &json).unwrap();
    assert_eq!(rebytes, bytes, "JSON roundtrip must be byte-stable");

    // Text format via the CLI library helpers.
    let text = tpt_proto_cli::binary_to_text(&pool, &desc, &bytes).unwrap();
    assert!(text.contains("ada"), "unexpected text: {text}");
    let rebytes2 = tpt_proto_cli::text_to_binary(&pool, &desc, &text).unwrap();
    assert_eq!(rebytes2, bytes, "text roundtrip must be byte-stable");

    // ---- Stage 4: incremental rebuild skips regeneration ---------------
    let out_rs = out_dir.join("user.rs");
    let before = std::fs::metadata(&out_rs).unwrap().modified().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(20));
    let includes = [root.join("proto")];
    compile_protos(&[proto_path], &includes, &out_dir, &config)
        .expect("second compile_protos failed");
    let after = std::fs::metadata(&out_rs).unwrap().modified().unwrap();
    assert_eq!(before, after, "unchanged inputs must not rewrite outputs");

    let _ = std::fs::remove_dir_all(&root);
}
