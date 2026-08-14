//! Example that generates the committed `tests/sample_generated.rs` so the
//! integration test can compile and round-trip the generated Rust code.
//!
//! Run with `cargo run -p tpt-proto-codegen-rust --example gen_sample`.

fn main() {
    let parsed =
        tpt_proto_language::parse_file("sample.proto", tpt_proto_codegen_rust::SAMPLE_PROTO);
    assert!(
        !parsed.diagnostics.has_errors(),
        "parse errors: {:?}",
        parsed.diagnostics.iter().collect::<Vec<_>>()
    );
    let res = tpt_proto_compiler::compile_set(&[parsed.file]);
    assert!(
        !res.diagnostics.has_errors(),
        "compile errors: {:?}",
        res.diagnostics.iter().collect::<Vec<_>>()
    );
    let code = tpt_proto_codegen_rust::generate(
        &res.set,
        &tpt_proto_codegen_rust::GenerateOptions {
            json: true,
            text: true,
            ..Default::default()
        },
    )
    .expect("codegen failed");

    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sample_generated.rs");
    std::fs::write(&out, code).expect("failed to write generated file");
    println!("wrote {}", out.display());
}
