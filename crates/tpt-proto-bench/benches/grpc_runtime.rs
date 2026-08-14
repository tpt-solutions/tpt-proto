//! gRPC *runtime* benchmarks (Phase 18, deferred items gated on Phase 14).
//!
//! These exercise the real HTTP/2 server + client runtime (in-tree, cleartext
//! h2c on a loopback TCP socket) for the four call patterns and the
//! cancellation / deadline / concurrency stress scenarios. The framing +
//! compression hot path is covered separately in `grpc.rs`.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::stream;
use futures::StreamExt;
use tokio::net::TcpListener;
use tokio::sync::Barrier;
use tokio::task::JoinSet;

use tpt_proto_core::scalar;
use tpt_proto_core::{Message, Reader};
use tpt_proto_grpc::client::{Client, Endpoint};
use tpt_proto_grpc::context::RpcContext;
use tpt_proto_grpc::metadata::Metadata;
use tpt_proto_grpc::method::MethodKind;
use tpt_proto_grpc::server::{Server, ServerBuilder};
use tpt_proto_grpc::status::Status;
use tpt_proto_grpc::transport::{ClientStream, ServerStream, Transport};

// ---------------------------------------------------------------------------
// Wire messages (manual, dependency-free) for the benchmark service.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Default)]
struct Echo {
    body: Vec<u8>,
    seq: i64,
}

impl Message for Echo {
    fn encode(&self, w: &mut tpt_proto_core::Writer) -> tpt_proto_core::Result<()> {
        scalar::encode_bytes(w, 1, &self.body);
        scalar::encode_int64(w, 2, self.seq);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
        loop {
            let tag = match r.read_tag() {
                Ok(t) => t,
                Err(_) => break,
            };
            match tag.field_number {
                1 => self.body = r.read_length_delimited()?.to_vec(),
                2 => self.seq = scalar::read_int64(r)?,
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
struct Count {
    n: i64,
}

impl Message for Count {
    fn encode(&self, w: &mut tpt_proto_core::Writer) -> tpt_proto_core::Result<()> {
        scalar::encode_int64(w, 1, self.n);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
        loop {
            let tag = match r.read_tag() {
                Ok(t) => t,
                Err(_) => break,
            };
            match tag.field_number {
                1 => self.n = scalar::read_int64(r)?,
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Service handler (raw-bytes dispatch over the four method kinds).
// ---------------------------------------------------------------------------

struct BenchService {
    methods: Vec<(String, MethodKind)>,
}

impl BenchService {
    fn new() -> Self {
        BenchService {
            methods: vec![
                ("/bench.BenchService/Unary".into(), MethodKind::Unary),
                ("/bench.BenchService/ServerStream".into(), MethodKind::ServerStreaming),
                ("/bench.BenchService/ClientStream".into(), MethodKind::ClientStreaming),
                ("/bench.BenchService/Bidi".into(), MethodKind::BidiStreaming),
            ],
        }
    }
}

#[async_trait]
impl tpt_proto_grpc::service::ServiceHandler for BenchService {
    fn full_name(&self) -> &str {
        "bench.BenchService"
    }
    fn methods(&self) -> Vec<(String, MethodKind)> {
        self.methods.clone()
    }
    async fn call_unary(
        &self,
        _method: &str,
        _ctx: RpcContext,
        req: Vec<u8>,
    ) -> Result<Vec<u8>, Status> {
        Ok(req) // echo
    }
    async fn call_server_streaming(
        &self,
        _method: &str,
        _ctx: RpcContext,
        req: Vec<u8>,
    ) -> Result<ServerStream<Vec<u8>>, Status> {
        let count = Count::decode(&req).map(|c| c.n).unwrap_or(1);
        let items: Vec<Result<Vec<u8>, Status>> = (0..count)
            .map(|i| Ok(Echo { body: req.clone(), seq: i }.encode_to_vec().unwrap()))
            .collect();
        Ok(Box::pin(stream::iter(items)))
    }
    async fn call_client_streaming(
        &self,
        _method: &str,
        _ctx: RpcContext,
        req: ClientStream<Vec<u8>>,
    ) -> Result<Vec<u8>, Status> {
        let mut n: i64 = 0;
        let mut s = req;
        while let Some(item) = s.next().await {
            let _m = item?;
            n += 1;
        }
        Ok(Count { n }.encode_to_vec().unwrap())
    }
    async fn call_bidi_streaming(
        &self,
        _method: &str,
        _ctx: RpcContext,
        req: ClientStream<Vec<u8>>,
    ) -> Result<ServerStream<Vec<u8>>, Status> {
        let mapped = req.map(|r| r.map(|m| m));
        Ok(Box::pin(mapped))
    }
}

// ---------------------------------------------------------------------------
// Harness: start a server on an ephemeral port, return its client channel.
// ---------------------------------------------------------------------------

async fn start_server() -> (Arc<Client>, std::net::SocketAddr) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server: Server = ServerBuilder::new()
        .max_concurrent_streams(1000)
        .max_concurrent_rpcs(10_000)
        .disable_keepalive()
        .build();
    server.add_service(BenchService::new());
    tokio::spawn(async move {
        let _ = server.serve_listener(listener).await;
    });
    let mut endpoint = Endpoint::from_shared(&format!("127.0.0.1:{}", addr.port())).unwrap();
    endpoint.max_message_size = 16 * 1024 * 1024;
    endpoint.connect_timeout = Duration::from_secs(5);
    let client = Arc::new(Client::new(endpoint));
    (client, addr)
}

fn payload(size: usize) -> Echo {
    Echo {
        body: (0..size).map(|i| ((i * 31 + 7) % 251) as u8).collect(),
        seq: 0,
    }
}

/// Scale an iteration count by the `TPT_BENCH_SCALE` env var (default 1.0).
/// Lets a quick smoke run use e.g. `TPT_BENCH_SCALE=0.01`.
fn scale(n: u64) -> u64 {
    match std::env::var("TPT_BENCH_SCALE")
        .ok()
        .and_then(|s| s.parse::<f64>().ok())
    {
        Some(f) if f > 0.0 => ((n as f64) * f).max(1.0) as u64,
        _ => n,
    }
}

// ---------------------------------------------------------------------------
// Benchmark scenarios.
// ---------------------------------------------------------------------------

async fn bench_unary_throughput() {
    let (client, _addr) = start_server().await;
    let path = "/bench.BenchService/Unary";
    let md = Metadata::new();

    for size in [64usize, 1024, 64 * 1024, 512 * 1024] {
        let msg = payload(size);
        let raw = msg.encode_to_vec().unwrap();
        let bytes = raw.len() as u64;
        let client = client.clone();
        let raw = raw.clone();
        let label = format!("grpc/runtime/unary/{size}");
        // Warmup.
        for _ in 0..200 {
            let _ = client.unary(&path, md.clone(), raw.clone()).await.unwrap();
        }
        let iters = scale(5_000u64);
        let start = std::time::Instant::now();
        for _ in 0..iters {
            let (resp, _) = client.unary(&path, md.clone(), raw.clone()).await.unwrap();
            std::hint::black_box(resp);
        }
        let elapsed = start.elapsed();
        let ns = elapsed.as_nanos() as f64 / iters as f64;
        let mb = (bytes * iters) as f64 / (ns / 1e9) / (1024.0 * 1024.0);
        println!(
            "{:<44} {:>9} it  {:>9.1} ns/it  {:>9.1} MB/s",
            label, iters, ns, mb
        );
    }
}

async fn bench_server_streaming() {
    let (client, _addr) = start_server().await;
    let path = "/bench.BenchService/ServerStream";
    let md = Metadata::new();

    for count in [10u64, 100, 1000] {
        let req = Count { n: count as i64 }.encode_to_vec().unwrap();
        let client = client.clone();
        let label = format!("grpc/runtime/server_stream/{count}");
        let warm = 50;
        for _ in 0..warm {
            let (mut s, _) = client.server_streaming(&path, md.clone(), req.clone()).await.unwrap();
            while let Some(_) = s.next().await {}
        }
        let iters = scale(1_000u64);
        let start = std::time::Instant::now();
        for _ in 0..iters {
            let (mut s, _) = client.server_streaming(&path, md.clone(), req.clone()).await.unwrap();
            let mut got = 0u64;
            while let Some(_) = s.next().await {
                got += 1;
            }
            std::hint::black_box(got);
        }
        let elapsed = start.elapsed();
        let ns = elapsed.as_nanos() as f64 / iters as f64;
        println!("{:<44} {:>9} it  {:>9.1} ns/it", label, iters, ns);
    }
}

async fn bench_client_streaming() {
    let (client, _addr) = start_server().await;
    let path = "/bench.BenchService/ClientStream";
    let md = Metadata::new();

    for count in [10u64, 100, 1000] {
        let items: Vec<Result<Vec<u8>, Status>> = (0..count)
            .map(|i| Ok(Echo { body: vec![1; 64], seq: i as i64 }.encode_to_vec().unwrap()))
            .collect();
        let client = client.clone();
        let label = format!("grpc/runtime/client_stream/{count}");
        let warm = 50;
        for _ in 0..warm {
            let s: ClientStream<Vec<u8>> = Box::pin(stream::iter(items.clone()));
            let (resp, _) = client.client_streaming(&path, md.clone(), s).await.unwrap();
            std::hint::black_box(resp);
        }
        let iters = scale(1_000u64);
        let start = std::time::Instant::now();
        for _ in 0..iters {
            let s: ClientStream<Vec<u8>> = Box::pin(stream::iter(items.clone()));
            let (resp, _) = client.client_streaming(&path, md.clone(), s).await.unwrap();
            std::hint::black_box(resp);
        }
        let elapsed = start.elapsed();
        let ns = elapsed.as_nanos() as f64 / iters as f64;
        println!("{:<44} {:>9} it  {:>9.1} ns/it", label, iters, ns);
    }
}

async fn bench_bidi_streaming() {
    let (client, _addr) = start_server().await;
    let path = "/bench.BenchService/Bidi";
    let md = Metadata::new();

    for count in [10u64, 100, 1000] {
        let items: Vec<Result<Vec<u8>, Status>> = (0..count)
            .map(|i| Ok(Echo { body: vec![1; 64], seq: i as i64 }.encode_to_vec().unwrap()))
            .collect();
        let client = client.clone();
        let label = format!("grpc/runtime/bidi/{count}");
        let warm = 50;
        for _ in 0..warm {
            let s: ClientStream<Vec<u8>> = Box::pin(stream::iter(items.clone()));
            let (mut r, _) = client.bidi_streaming(&path, md.clone(), s).await.unwrap();
            while let Some(_) = r.next().await {}
        }
        let iters = scale(1_000u64);
        let start = std::time::Instant::now();
        for _ in 0..iters {
            let s: ClientStream<Vec<u8>> = Box::pin(stream::iter(items.clone()));
            let (mut r, _) = client.bidi_streaming(&path, md.clone(), s).await.unwrap();
            let mut got = 0u64;
            while let Some(_) = r.next().await {
                got += 1;
            }
            std::hint::black_box(got);
        }
        let elapsed = start.elapsed();
        let ns = elapsed.as_nanos() as f64 / iters as f64;
        println!("{:<44} {:>9} it  {:>9.1} ns/it", label, iters, ns);
    }
}

/// Many concurrent unary RPCs issued in parallel; measures fan-out throughput
/// under HTTP/2 multiplexing rather than per-call latency.
async fn bench_concurrent_streams() {
    let (client, _addr) = start_server().await;
    let path = "/bench.BenchService/Unary";
    let md = Metadata::new();
    let raw = payload(256).encode_to_vec().unwrap();

    for concurrency in [64u64, 256, 1024] {
        let label = format!("grpc/runtime/concurrent/{concurrency}");
        let warm = 1_000u64;
        let barrier = Arc::new(Barrier::new(concurrency as usize));
        let md = md.clone();
        for _ in 0..warm {
            let b = barrier.clone();
            let c = client.clone();
            let r = raw.clone();
            let md = md.clone();
            tokio::spawn(async move {
                b.wait().await;
                let _ = c.unary(&path, md, r).await.unwrap();
            });
        }
        let iters = scale(10_000u64);
        let start = std::time::Instant::now();
        for _ in 0..iters {
            let mut set: JoinSet<()> = JoinSet::new();
            for _ in 0..concurrency {
                let b = barrier.clone();
                let c = client.clone();
                let r = raw.clone();
                let md = md.clone();
                set.spawn(async move {
                    b.wait().await;
                    let _ = c.unary(&path, md, r).await.unwrap();
                });
            }
            while set.join_next().await.is_some() {}
        }
        let elapsed = start.elapsed();
        let total = iters * concurrency;
        let ns = elapsed.as_nanos() as f64 / total as f64;
        let rps = total as f64 / elapsed.as_secs_f64();
        println!(
            "{:<44} {:>9} it  {:>9.1} ns/call  {:>9.1} calls/s",
            label, total, ns, rps
        );
    }
}

/// Cancellation storm: spawn unary RPCs and drop them before completion,
/// measuring how fast the runtime reclaims streams. We model the "drop future"
/// cancellation path by racing each call against an immediate timeout.
async fn bench_cancellation_storm() {
    let (client, _addr) = start_server().await;
    let path = "/bench.BenchService/Unary";
    let md = Metadata::new();
    let raw = payload(4096).encode_to_vec().unwrap();

    let iters = scale(20_000u64);
    let label = "grpc/runtime/cancel_storm".to_string();
    let start = std::time::Instant::now();
    for _ in 0..iters {
        let c = client.clone();
        let r = raw.clone();
        // Cancel almost immediately: the select drops the RPC future.
        tokio::select! {
            _ = c.unary(&path, md.clone(), r) => {}
            _ = tokio::time::sleep(Duration::from_nanos(1)) => {}
        }
    }
    let elapsed = start.elapsed();
    let ns = elapsed.as_nanos() as f64 / iters as f64;
    println!("{:<44} {:>9} it  {:>9.1} ns/it", label, iters, ns);
}

/// Deadline-expiry storm: send unary RPCs with a tiny `grpc-timeout` so the
/// server returns DEADLINE_EXCEEDED; measures per-call terminal-status cost.
async fn bench_deadline_storm() {
    let (client, _addr) = start_server().await;
    let path = "/bench.BenchService/Unary";
    let mut md = Metadata::new();
    let _ = md.insert_text("grpc-timeout", "1u"); // 1 microsecond; server will exceed it
    let raw = payload(4096).encode_to_vec().unwrap();

    let iters = scale(5_000u64);
    let label = "grpc/runtime/deadline_storm".to_string();
    let start = std::time::Instant::now();
    let mut seen_deadline = 0u64;
    for _ in 0..iters {
        let res = client.unary(&path, md.clone(), raw.clone()).await;
        if let Err(s) = &res {
            if s.code == tpt_proto_grpc::status::Code::DeadlineExceeded {
                seen_deadline += 1;
            }
        }
        let _ = std::hint::black_box(res);
    }
    let elapsed = start.elapsed();
    let ns = elapsed.as_nanos() as f64 / iters as f64;
    println!(
        "{:<44} {:>9} it  {:>9.1} ns/it  ({} deadline-exceeded)",
        label, iters, ns, seen_deadline
    );
}

fn main() {
    println!("=== grpc runtime (http/2 h2c loopback) ===");
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            bench_unary_throughput().await;
            bench_server_streaming().await;
            bench_client_streaming().await;
            bench_bidi_streaming().await;
            bench_concurrent_streams().await;
            bench_cancellation_storm().await;
            bench_deadline_storm().await;
        });
}
