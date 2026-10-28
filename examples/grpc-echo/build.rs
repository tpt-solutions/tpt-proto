//! Compiles `proto/greet.proto` into Rust (messages + gRPC server trait /
//! client stub) as part of the crate build.

fn main() {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    tpt_proto_build::compile_protos(
        &["proto/greet.proto".into()],
        &["proto".into()],
        &out_dir,
        &tpt_proto_build::BuildConfig {
            grpc: true,
            ..Default::default()
        },
    )
    .expect("protobuf compilation failed");
}
