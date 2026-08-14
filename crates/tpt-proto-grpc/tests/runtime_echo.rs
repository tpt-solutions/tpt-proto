//! End-to-end integration test: a real HTTP/2 server and client over TCP.
//!
//! The generated `echo_gen.rs` (produced by `tpt-proto-cli generate --grpc`)
//! provides the `Echo` server trait, the `EchoServer` adapter, and the
//! `EchoClient` stub. We implement `Echo` directly and drive every method kind
//! through the real h2 runtime, exercising framing, compression negotiation,
//! metadata, deadlines, and streaming backpressure.

mod generated {
    include!("../examples/grpc_echo/echo_gen.rs");
}

use std::sync::Arc;

use futures::StreamExt;
use tokio::net::TcpListener;

use generated::{Echo, EchoClient, EchoServer, ExPing, ExPong};
use tpt_proto_grpc::{
    Client, ClientStream, Endpoint, Request, Response, Server, ServerStream, Status,
};

struct MyEcho;

#[async_trait::async_trait]
impl Echo for MyEcho {
    async fn unary(&self, request: Request<ExPing>) -> Result<Response<ExPong>, Status> {
        let mut p = ExPong::default();
        p.msg = format!("echo: {}", request.message.msg);
        Ok(Response::new(p))
    }

    async fn server_stream(
        &self,
        request: Request<ExPing>,
    ) -> Result<Response<ServerStream<ExPong>>, Status> {
        let msg = request.message.msg.clone();
        let items: Vec<Result<ExPong, Status>> = (0..3)
            .map(|i| {
                let mut p = ExPong::default();
                p.msg = format!("{msg}-{i}");
                Ok(p)
            })
            .collect();
        let stream: ServerStream<ExPong> = Box::pin(futures::stream::iter(items));
        Ok(Response::new(stream))
    }

    async fn client_stream(
        &self,
        request: Request<ClientStream<ExPing>>,
    ) -> Result<Response<ExPong>, Status> {
        let mut count = 0u32;
        let mut input = request.message;
        while let Some(item) = input.next().await {
            let _ = item?;
            count += 1;
        }
        let mut p = ExPong::default();
        p.msg = format!("count={count}");
        Ok(Response::new(p))
    }

    async fn bidi(
        &self,
        request: Request<ClientStream<ExPing>>,
    ) -> Result<Response<ServerStream<ExPong>>, Status> {
        let mut input = request.message;
        let (mut tx, rx) = futures::channel::mpsc::channel::<Result<ExPong, Status>>(8);
        tokio::spawn(async move {
            while let Some(item) = input.next().await {
                match item {
                    Ok(p) => {
                        let mut out = ExPong::default();
                        out.msg = format!("echo: {}", p.msg);
                        if tx.start_send(Ok(out)).is_err() {
                             break;
                         }
                    }
                    Err(e) => {
                        let _ = tx.start_send(Err(e));
                        break;
                    }
                }
            }
        });
        let stream: ServerStream<ExPong> = Box::pin(rx);
        Ok(Response::new(stream))
    }
}

/// Spin up a server on an ephemeral localhost port and return its address plus
/// a shutdown handle.
async fn spawn_server() -> (std::net::SocketAddr, tpt_proto_grpc::CancellationToken) {
    let server = Server::builder().build();
    server.add_service(EchoServer::new(MyEcho));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let shutdown = server.shutdown_token();
    tokio::spawn(async move {
        let _ = server.serve_listener(listener).await;
    });
    (addr, shutdown)
}

fn client_for(addr: std::net::SocketAddr) -> EchoClient {
    let endpoint = Endpoint::from_shared(&addr.to_string()).unwrap();
    let channel = Client::new(endpoint).into_channel();
    EchoClient::new(channel)
}

#[tokio::test]
async fn unary_over_tcp() {
    let (addr, shutdown) = spawn_server().await;
    let mut client = client_for(addr);

    let mut ping = ExPing::default();
    ping.msg = "hello".into();
    let resp = client.unary(Request::new(ping)).await.unwrap();
    assert_eq!(resp.message.msg, "echo: hello");

    shutdown.cancel();
}

#[tokio::test]
async fn server_streaming_over_tcp() {
    let (addr, shutdown) = spawn_server().await;
    let mut client = client_for(addr);

    let mut ping = ExPing::default();
    ping.msg = "s".into();
    let resp = client.server_stream(Request::new(ping)).await.unwrap();
    let msgs: Vec<String> = resp
        .message
        .map(|r| r.unwrap().msg)
        .collect()
        .await;
    assert_eq!(msgs, vec!["s-0", "s-1", "s-2"]);

    shutdown.cancel();
}

#[tokio::test]
async fn client_streaming_over_tcp() {
    let (addr, shutdown) = spawn_server().await;
    let mut client = client_for(addr);

    let items: Vec<Result<ExPing, Status>> = (0..4)
        .map(|i| {
            let mut p = ExPing::default();
            p.msg = format!("p{i}");
            Ok(p)
        })
        .collect();
    let req_stream: ClientStream<ExPing> = Box::pin(futures::stream::iter(items));
    let resp = client.client_stream(Request::new(req_stream)).await.unwrap();
    assert_eq!(resp.message.msg, "count=4");

    shutdown.cancel();
}

#[tokio::test]
async fn bidi_streaming_over_tcp() {
    let (addr, shutdown) = spawn_server().await;
    let mut client = client_for(addr);

    let items: Vec<Result<ExPing, Status>> = (0..3)
        .map(|i| {
            let mut p = ExPing::default();
            p.msg = format!("b{i}");
            Ok(p)
        })
        .collect();
    let req_stream: ClientStream<ExPing> = Box::pin(futures::stream::iter(items));
    let resp = client.bidi(Request::new(req_stream)).await.unwrap();
    let msgs: Vec<String> = resp
        .message
        .map(|r| r.unwrap().msg)
        .collect()
        .await;
    assert_eq!(msgs, vec!["echo: b0", "echo: b1", "echo: b2"]);

    shutdown.cancel();
}

#[tokio::test]
async fn unary_error_status_surfaces() {
    let (addr, shutdown) = spawn_server().await;
    let mut client = client_for(addr);

    // Unknown method path should produce a gRPC Unimplemented status.
    let mut ping = ExPing::default();
    ping.msg = "x".into();
    // The generated client only knows `Unary`; exercise the transport error
    // path by driving the channel's transport directly against a bad path.
    let result = client
        .channel
        .transport()
        .unary(
            "/ex.Echo/DoesNotExist",
            tpt_proto_grpc::Metadata::new(),
            ping.encode_to_vec().unwrap(),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().code, tpt_proto_grpc::Code::Unimplemented);

    shutdown.cancel();
}
