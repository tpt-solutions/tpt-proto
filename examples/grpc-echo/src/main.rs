//! End-to-end tpt-proto demo: binary roundtrip + real TCP gRPC call.
//!
//! The `generated` module below is produced by `build.rs` from
//! `proto/greet.proto` — nothing here is hand-written or checked in.

// Generated code: skip clippy style lints and the rustc unused/dead-code
// warnings that stubs and unused message builders trigger.
#[allow(
    clippy::all,
    dead_code,
    non_camel_case_types,
    unused_imports,
    unused_variables
)]
mod generated {
    include!(concat!(env!("OUT_DIR"), "/greet.rs"));
}

use std::net::SocketAddr;

// Message types are prefixed with the PascalCase package (`greet.v1`);
// re-alias them for readability.
use generated::{
    GreetV1HelloReply as HelloReply, GreetV1HelloRequest as HelloRequest, Greeter, GreeterClient,
    GreeterServer,
};
use tpt_proto_grpc::{Client, Endpoint, Request, Response, Server, Status};

/// The user-implemented service logic, exactly like implementing a tonic
/// service trait.
struct GreeterImpl;

#[async_trait::async_trait]
impl Greeter for GreeterImpl {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        let reply = HelloReply {
            message: format!("Hello, {}!", request.message.name),
            ..Default::default()
        };
        Ok(Response::new(reply))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ------------------------------------------------------------------
    // 1) Plain message roundtrip through the binary wire format.
    // ------------------------------------------------------------------
    let req = HelloRequest {
        name: "wire".into(),
        ..Default::default()
    };
    let bytes = req.encode_to_vec()?;
    let decoded = HelloRequest::decode(&bytes)?;
    assert_eq!(decoded.name, "wire");
    println!(
        "binary roundtrip ok: {:?} ({} bytes on the wire)",
        decoded.name,
        bytes.len()
    );

    // ------------------------------------------------------------------
    // 2) A real gRPC server + client pair over TCP HTTP/2 (h2c).
    // ------------------------------------------------------------------
    let server = Server::builder().build();
    server.add_service(GreeterServer::new(GreeterImpl));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr: SocketAddr = listener.local_addr()?;
    let shutdown = server.shutdown_token();
    tokio::spawn(async move {
        let _ = server.serve_listener(listener).await;
    });
    println!("grpc server listening on {addr}");

    let endpoint = Endpoint::from_shared(&addr.to_string())?;
    let channel = Client::new(endpoint).into_channel();
    let client = GreeterClient::new(channel);

    let hello = HelloRequest {
        name: "Ada".into(),

        ..Default::default()
    };
    let resp = client.say_hello(Request::new(hello)).await?;
    println!("grpc reply: {}", resp.message.message);
    assert_eq!(resp.message.message, "Hello, Ada!");

    shutdown.cancel();
    println!("done");
    Ok(())
}
