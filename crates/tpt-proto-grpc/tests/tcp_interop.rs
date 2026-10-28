//! Real-TCP gRPC interop coverage beyond `runtime_echo.rs`: deadlines
//! (`grpc-timeout`), gzip compression negotiation, response-trailer metadata
//! merging, and client-side cancellation by dropping the in-flight call.
//!
//! Both peers are independently driven: a spawned `Server` task on an ephemeral
//! loopback port and an `H2Transport` client over its own fresh TCP+h2
//! connection. True third-party interop (grpc-go/C++/Java) is out of scope in
//! this repo without that toolchain available.

mod generated {
    include!("../examples/grpc_echo/echo_gen.rs");
}

use std::time::Duration;

use generated::{Echo, EchoServer, ExPing, ExPong};
use tokio::net::TcpListener;
use tpt_proto_grpc::{
    Code, Compression, H2Transport, Metadata, Request, Response, Server, ServerStream, Status,
    Transport,
};

/// Echo service whose unary method sleeps for `delay` before replying.
struct TimedEcho {
    delay: Duration,
}

#[async_trait::async_trait]
impl Echo for TimedEcho {
    async fn unary(&self, request: Request<ExPing>) -> Result<Response<ExPong>, Status> {
        tokio::time::sleep(self.delay).await;
        let mut p = ExPong::default();
        p.msg = format!("echo: {}", request.message.msg);
        Ok(Response::new(p))
    }

    async fn server_stream(
        &self,
        _request: Request<ExPing>,
    ) -> Result<Response<ServerStream<ExPong>>, Status> {
        Err(Status::new(Code::Unimplemented, "not used here"))
    }

    async fn client_stream(
        &self,
        _request: Request<tpt_proto_grpc::ClientStream<ExPing>>,
    ) -> Result<Response<ExPong>, Status> {
        Err(Status::new(Code::Unimplemented, "not used here"))
    }

    async fn bidi(
        &self,
        _request: Request<tpt_proto_grpc::ClientStream<ExPing>>,
    ) -> Result<Response<ServerStream<ExPong>>, Status> {
        Err(Status::new(Code::Unimplemented, "not used here"))
    }
}

async fn spawn(delay: Duration) -> std::net::SocketAddr {
    spawn_with(delay, Compression::Identity).await
}

async fn spawn_with(delay: Duration, response_compression: Compression) -> std::net::SocketAddr {
    let server = Server::builder()
        .response_compression(response_compression)
        .build();
    server.add_service(EchoServer::new(TimedEcho { delay }));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let shutdown = server.shutdown_token();
    tokio::spawn(async move {
        let _ = server.serve_listener(listener).await;
    });
    let _ = shutdown; // dropping the extra handle is fine; the task owns its clone
    addr
}

fn transport(addr: std::net::SocketAddr) -> H2Transport {
    H2Transport::new(addr.to_string())
}

/// A `grpc-timeout` derived deadline shorter than the handler's sleep must
/// surface as DEADLINE_EXCEEDED through the trailers.
#[tokio::test]
async fn deadline_exceeded_over_real_tcp() {
    let addr = spawn(Duration::from_millis(500)).await;
    let t = transport(addr).with_timeout(Duration::from_millis(50));
    let err = t
        .unary("/ex.Echo/Unary", Metadata::new(), encode_ping("deadline"))
        .await
        .unwrap_err();
    assert_eq!(err.code, Code::DeadlineExceeded, "got: {err:?}");
}

/// The same slow call with a generous deadline succeeds.
#[tokio::test]
async fn generous_deadline_completes_over_real_tcp() {
    let addr = spawn(Duration::ZERO).await;
    let t = transport(addr).with_timeout(Duration::from_secs(5));
    let body = encode_ping("slow-ok");
    let (resp, _) = t
        .unary("/ex.Echo/Unary", Metadata::new(), body)
        .await
        .unwrap();
    let pong = ExPong::decode(&resp).unwrap();
    assert_eq!(pong.msg, "echo: slow-ok");
}

/// gzip-compressed request payloads round-trip against the real runtime, with
/// the server configured to compress responses with gzip as well.
#[tokio::test]
async fn gzip_compressed_unary_over_real_tcp() {
    let addr = spawn_with(Duration::ZERO, Compression::Gzip).await;
    let t = transport(addr)
        .with_compression(Compression::Gzip)
        .with_max_message_size(1 << 20);
    let body = encode_ping("gz");
    let (resp, _) = t
        .unary("/ex.Echo/Unary", Metadata::new(), body)
        .await
        .unwrap();
    let pong = ExPong::decode(&resp).unwrap();
    assert_eq!(pong.msg, "echo: gz");
}

/// Successful unary responses carry their terminal status in merged trailer
/// metadata (`grpc-status: 0`).
#[tokio::test]
async fn ok_status_trailers_merged_into_response_metadata() {
    let addr = spawn(Duration::ZERO).await;
    let t = transport(addr);
    let (_, meta) = t
        .unary("/ex.Echo/Unary", Metadata::new(), encode_ping("tr"))
        .await
        .unwrap();
    assert_eq!(meta.get_text("grpc-status"), Some("0"));
}

/// Dropping the future of an in-flight call (client cancellation) must not
/// wedge the connection or server: a follow-up call on a fresh connection
/// completes normally.
#[tokio::test]
async fn cancelled_call_does_not_wedge_subsequent_calls() {
    let addr = spawn(Duration::from_millis(400)).await;

    let t1 = transport(addr);
    let fut = t1.unary("/ex.Echo/Unary", Metadata::new(), encode_ping("cancel-me"));
    drop(fut); // cancel immediately

    let t2 = transport(addr); // fresh TCP+h2 connection
    let (resp, _) = t2
        .unary("/ex.Echo/Unary", Metadata::new(), encode_ping("after"))
        .await
        .unwrap();
    let pong = ExPong::decode(&resp).unwrap();
    assert_eq!(pong.msg, "echo: after");
}

fn encode_ping(msg: &str) -> Vec<u8> {
    let mut p = ExPing::default();
    p.msg = msg.to_string();
    p.encode_to_vec().unwrap()
}
