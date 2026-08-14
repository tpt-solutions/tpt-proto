//! Tests for gRPC server-trait / client-stub code generation.

use tpt_proto_codegen_rust::{generate_from_source, GenerateOptions};

const SERVICE_PROTO: &str = r#"
syntax = "proto3";
package ex;

message Ping { string msg = 1; }
message Pong { string msg = 1; }

service Echo {
  rpc Unary(Ping) returns (Pong);
  rpc Watch(Ping) returns (stream Pong);
  rpc Upload(stream Ping) returns (Pong);
  rpc Chat(stream Ping) returns (stream Pong);
}
"#;

#[test]
fn generates_grpc_server_trait_and_client() {
    let opts = GenerateOptions {
        module_per_package: false,
        grpc: true,
        json: false,
        text: false,
    };
    let code = generate_from_source("ex.proto", SERVICE_PROTO, &opts).unwrap();

    // Server trait for all four method kinds.
    assert!(code.contains("pub trait Echo: Send + Sync"));
    assert!(code.contains("async fn unary(&self, request: __grpc::Request<ExPing>)"));
    assert!(code.contains("async fn watch(&self, request: __grpc::Request<ExPing>) -> std::result::Result<__grpc::Response<__grpc::ServerStream<ExPong>>, __grpc::Status>"));
    assert!(code.contains("async fn upload(&self, request: __grpc::Request<__grpc::ClientStream<ExPing>>)"));
    assert!(code.contains("async fn chat(&self, request: __grpc::Request<__grpc::ClientStream<ExPing>>) -> std::result::Result<__grpc::Response<__grpc::ServerStream<ExPong>>, __grpc::Status>"));

    // Client stub.
    assert!(code.contains("pub struct EchoClient"));
    assert!(code.contains("pub fn new(channel: __grpc::Channel) -> Self"));
    assert!(code.contains(".unary::<ExPing, ExPong>("));
    // Streaming client methods are scaffolded to return Unimplemented.
    assert!(code.contains("__grpc::Status::new(__grpc::Code::Unimplemented"));

    // Runtime import present.
    assert!(code.contains("use tpt_proto_grpc as __grpc;"));
}

#[test]
fn grpc_disabled_emits_placeholder_trait() {
    let opts = GenerateOptions {
        module_per_package: false,
        grpc: false,
        json: false,
        text: false,
    };
    let code = generate_from_source("ex.proto", SERVICE_PROTO, &opts).unwrap();
    assert!(code.contains("pub trait Echo"));
    assert!(!code.contains("__grpc::Request"));
    assert!(!code.contains("pub struct EchoClient"));
}
