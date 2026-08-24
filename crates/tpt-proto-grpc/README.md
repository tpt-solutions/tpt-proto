# tpt-proto-grpc

**gRPC protocol, server, and client runtime — built on tpt-proto.**

`tpt-proto-grpc` implements the gRPC protocol layer on top of tpt-proto message
serialization: message framing, metadata, status/trailers, deadlines &
cancellation, compression, the four method kinds, and the RPC context shared by
generated server traits and client stubs. An HTTP/2 transport (`H2Transport`)
and streaming runtime provide the network layer, alongside observability, the
health-checking protocol, server reflection, and security configuration (TLS +
auth/authz + peer identity).

## What's included

- **Protocol types**: `Metadata`, `Status`/`Code`, `RpcContext`, `Request`/
  `Response`, framing (`codec`, `framed`), `Compression`.
- **Method model**: `Method`, `MethodKind`, `Service`, `build_service`.
- **Server**: `Server`/`ServerBuilder`/`ServerConfig`, `ServiceHandler`,
  acceptors, health service (`health`), and reflection service (`reflection`).
- **Client**: `Client`/`Endpoint`, load balancing (`LoadBalancer`,
  `StaticLoadBalancer`), retries (`RetryPolicy`), `Channel`,
  `ClientStream`/`ServerStream`.
- **Transport**: `Transport` trait + `H2Transport` (HTTP/2).
- **Cross-cutting**: `CancellationToken`, `timeout` parsing, `interceptor`s,
  `observability` (metrics/tracing/logging), and `security` (TLS, bearer/PEM
  auth, RBAC authorizers).
- Helpers: `build_path` / `parse_path` for `/package.Service/Method`.

## Installation

```toml
[dependencies]
tpt-proto-grpc = "0.1"
```

Enable `tls` for optional TLS client support in the debug CLI (pulls in
`rustls` / `tokio-rustls`).

## Quick start

```rust,no_run
use tpt_proto_grpc::{Server, ServerBuilder, Status, RpcContext, Request, Response};

async fn hello(_ctx: RpcContext<'_>, req: Request<String>) -> Result<Response<String>, Status> {
    Ok(Response::new(format!("hi, {}", req.into_inner())))
}
```

## Status

Part of the in-progress `tpt-proto` workspace. The HTTP/2 transport and
streaming runtime are functional; see `../README.md` for the overall roadmap.

## License

Licensed under either of MIT or Apache-2.0 at your option.
