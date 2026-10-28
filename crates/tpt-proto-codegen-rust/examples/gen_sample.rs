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

    // Generator output is not held to the workspace's clippy style: these
    // lints fire on the emitted code shape (`*(&x)`, `Self { inner: T::default() }`,
    // `SampleWithOneof_Choice`), and reshaping every emitter for them is not
    // worth it. Written after the `@generated` marker so tooling still sees it.
    let (marker, rest) = code
        .split_once('\n')
        .expect("generated code has a header line");
    let code = format!(
        "{marker}\n#![allow(\n    clippy::deref_addrof,\n    clippy::needless_borrow,\n    clippy::needless_borrows_for_generic_args,\n    clippy::new_without_default,\n    clippy::redundant_closure,\n    clippy::unnecessary_cast,\n    clippy::upper_case_acronyms,\n    dead_code,\n    non_camel_case_types\n)]\n{rest}"
    );

    let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/sample_generated.rs");
    std::fs::write(&out, code).expect("failed to write generated file");
    println!("wrote {}", out.display());
}
