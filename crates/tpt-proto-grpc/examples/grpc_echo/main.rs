//! Runnable example: the gRPC codegen output used end-to-end.
//!
//! `echo_gen.rs` is produced by `tpt-proto-cli generate --grpc` from
//! `examples/echo.proto`. Here we include it verbatim, implement the generated
//! `Echo` server trait, wire it behind a mock in-process [`Transport`], and
//! drive the generated `EchoClient` through a [`Channel`] — exercising exactly
//! the code a user would compile and run.

mod generated {
    include!("echo_gen.rs");
}

use std::sync::Arc;

use async_trait::async_trait;
use generated::{Echo, EchoClient, ExPing, ExPong};
use tpt_proto_core::Message;
use tpt_proto_grpc::{
    Channel, ClientStream, Code, Metadata, Request, Response, ServerStream, Status, Transport,
};

/// A trivial server that echoes the request message.
struct MyEcho;

#[async_trait]
impl Echo for MyEcho {
    async fn unary(
        &self,
        request: Request<ExPing>,
    ) -> Result<Response<ExPong>, Status> {
        let mut pong = ExPong::default();
        pong.msg = format!("echo: {}", request.message.msg);
        Ok(Response::new(pong))
    }

    async fn server_stream(
        &self,
        _request: Request<ExPing>,
    ) -> Result<Response<ServerStream<ExPong>>, Status> {
        Err(Status::new(Code::Unimplemented, "not used in this example"))
    }

    async fn client_stream(
        &self,
        _request: Request<ClientStream<ExPing>>,
    ) -> Result<Response<ExPong>, Status> {
        Err(Status::new(Code::Unimplemented, "not used in this example"))
    }

    async fn bidi(
        &self,
        _request: Request<ClientStream<ExPing>>,
    ) -> Result<Response<ServerStream<ExPong>>, Status> {
        Err(Status::new(Code::Unimplemented, "not used in this example"))
    }
}

/// In-process transport that dispatches unary calls to a server impl.
struct LocalTransport {
    server: Arc<dyn Echo>,
}

#[async_trait]
impl Transport for LocalTransport {
    async fn unary(
        &self,
        path: &str,
        _metadata: Metadata,
        message: Vec<u8>,
    ) -> Result<(Vec<u8>, Metadata), Status> {
        assert_eq!(path, "/ex.Echo/Unary");
        let req = ExPing::decode(&message)
            .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        let resp = self.server.unary(Request::new(req)).await?;
        let bytes = resp
            .message
            .encode_to_vec()
            .map_err(|e| Status::new(Code::Internal, e.to_string()))?;
        Ok((bytes, Metadata::new()))
    }

    async fn server_streaming(
        &self,
        _path: &str,
        _metadata: Metadata,
        _message: Vec<u8>,
    ) -> Result<(ServerStream<Vec<u8>>, Metadata), Status> {
        Err(Status::new(Code::Unimplemented, "not used in this example"))
    }

    async fn client_streaming(
        &self,
        _path: &str,
        _metadata: Metadata,
        _stream: tpt_proto_grpc::ClientStream<Vec<u8>>,
    ) -> Result<(Vec<u8>, Metadata), Status> {
        Err(Status::new(Code::Unimplemented, "not used in this example"))
    }

    async fn bidi_streaming(
        &self,
        _path: &str,
        _metadata: Metadata,
        _stream: tpt_proto_grpc::ClientStream<Vec<u8>>,
    ) -> Result<(ServerStream<Vec<u8>>, Metadata), Status> {
        Err(Status::new(Code::Unimplemented, "not used in this example"))
    }
}

fn main() {
    let server = Arc::new(MyEcho);
    let channel = Channel::new(Arc::new(LocalTransport { server }));
    let mut client = EchoClient::new(channel);

    let mut ping = ExPing::default();
    ping.msg = "hello, gRPC".into();

    let resp = futures::executor::block_on(client.unary(Request::new(ping))).unwrap();
    println!("client received: {}", resp.message.msg);
    assert_eq!(resp.message.msg, "echo: hello, gRPC");
}
