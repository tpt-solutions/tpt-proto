# Migrating from prost / tonic to tpt-proto

This guide maps the prost/tonic concepts you know onto their tpt-proto
equivalents. It is written for teams using `prost` + `prost-build` for message
code generation and `tonic` + `tonic-build` for gRPC.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation and is not affiliated with the prost, tonic,
> or protobuf projects.

## The core difference

| | prost / tonic | tpt-proto |
| --- | --- | --- |
| Code generation model | Derive macros (`#[derive(Message)]`) on structs you write, *or* `prost_build` output compiled into your crate | A standalone generator (`tpt-proto-codegen-rust`), driven from `build.rs` via `tpt-proto-build` or ahead-of-time via `tpt-proto-cli generate` |
| Runtime dependency | `prost` (+ `bytes`) | `tpt-proto-core` |
| gRPC runtime | `tonic` (hyper/tower stack) | `tpt-proto-grpc` (own HTTP/2 implementation on `h2`) |
| Schema source of truth | Your Rust struct definitions (derive) or `.proto` files (build) | Always `.proto` files |

With tpt-proto you never hand-write message structs; they are always generated
from `.proto`, so schema and code cannot drift.

## Cargo.toml swap

Before:

```toml
[dependencies]
prost = "0.13"
tonic = "0.12"

[build-dependencies]
tonic-build = "0.12"
```

After:

```toml
[dependencies]
tpt-proto-core = "0"    # messages + wire format
tpt-proto-grpc = "0"    # only if you use gRPC

[build-dependencies]
tpt-proto-build = "0"   # .proto -> Rust (includes the codegen engine)
```

Note that `tpt-proto-codegen-rust` is **not** a direct dependency: it runs
inside `tpt-proto-build`. Generated code depends only on `tpt-proto-core`
(plus `tpt-proto-grpc` when services are generated).

## build.rs comparison

```rust
// prost-build / tonic-build
fn main() {
    tonic_build::configure()
        .compile_protos(&["proto/hello.proto"], &["proto"])
        .unwrap();
}
```

```rust
// tpt-proto-build — simple form (defaults)
fn main() {
    tpt_proto_build::compile_protos_simple(
        &["proto/hello.proto".into()],
        &["proto".into()],
    )
    .unwrap();
}
```

```rust
// tpt-proto-build — full form (gRPC stubs enabled)
fn main() {
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    tpt_proto_build::compile_protos(
        &["proto/hello.proto".into()],
        &["proto".into()],
        &out_dir,
        &tpt_proto_build::BuildConfig { grpc: true, ..Default::default() },
    )
    .unwrap();
}
```

Both tpt-proto entry points emit `cargo:rerun-if-changed` for every input
`.proto`, import, and include directory automatically.

Including the generated code:

```rust
// tpt-proto: one <file-stem>.rs per input, plus a mod.rs listing them all
include!(concat!(env!("OUT_DIR"), "/hello.rs"));
// or include everything at once:
include!(concat!(env!("OUT_DIR"), "/mod.rs"));
```

## Message API mapping

| prost | tpt-proto |
| --- | --- |
| derive `Message` struct | generated `#[derive(Debug, Clone, PartialEq, Default)]` + `impl Message` |
| `msg.encode_to_vec()` → `Vec<u8>` (infallible) | `msg.encode_to_vec()?` → `Result<Vec<u8>>` |
| `Msg::decode(buf)?` | `Msg::decode(buf)?` (same shape) |
| unknown fields dropped | preserved in `unknown_fields: UnknownFieldSet`, re-emitted on encode |
| `Option<T>` explicit presence | same: proto3 `optional`/proto2 optional → `Option<T>`; proto3 implicit/proto2 required → plain `T` |
| oneofs as nested module + enum | single idiomatic Rust enum field |
| no builders | generated `XxxBuilder` with required-field validation (`try_build()` / `build()`) |

Field types map 1:1 by proto type (`int32`→`i32`, `uint64`→`u64`,
`string`→`String`, `bytes`→`Vec<u8>`, `map<K,V>`→`HashMap<K,V>`, repeated
scalars→`Vec<T>` with packed encoding). Well-known types live in
`tpt-proto-wkt` (`Timestamp`, `Duration`, `Any`, `Struct`/`Value`/`ListValue`,
wrappers, `FieldMask`, `Empty`) with binary + JSON semantics — the role
`prost-types` plays for prost.

## gRPC: tonic ↔ tpt-proto-grpc

### Server

```rust
// tonic
let addr = "0.0.0.0:50051".parse()?;
Server::builder()
    .add_service(GreeterServer::new(greeter))
    .serve(addr)
    .await?;
```

```rust
// tpt-proto-grpc
use std::net::SocketAddr;
let server = tpt_proto_grpc::Server::builder().build();
server.add_service(GreeterServer::new(greeter));
server.serve("0.0.0.0:50051".parse::<SocketAddr>()?).await?;
```

### Client

```rust
// tonic
let mut client = GreeterClient::connect("http://localhost:50051").await?;
let resp = client.say_hello(Request::new(req)).await?;
```

```rust
// tpt-proto-grpc
let endpoint = Endpoint::from_shared("localhost:50051")?;
let channel = Client::new(endpoint).into_channel();
let mut client = GreeterClient::new(channel);
let resp = client.say_hello(Request::new(req)).await?;
```

### Service trait

Both generators emit an async-trait with the same method shape; tpt-proto uses
the PascalCase service name for the trait:

```rust
#[async_trait]
pub trait Greeter: Send + Sync + 'static {
    async fn say_hello(&self, request: Request<HelloRequest>)
        -> Result<Response<HelloReply>, Status>;
}
```

Streaming shapes:

| tonic | tpt-proto-grpc |
| --- | --- |
| server streaming: `Response<Streaming<T>>` | `Response<ServerStream<T>>` (boxed `Stream<Item = Result<T, Status>> + Send`) |
| client streaming: `Request<impl Stream>` | `Request<ClientStream<T>>` |
| bidi combines both | same combination |

Status/errors: `tonic::Status` ↔ `tpt_proto_grpc::Status` with
`tpt_proto_grpc::Code` (standard gRPC codes); rich error details compatible
with `google.rpc.Status` are supported. Metadata:
`tonic::metadata::MetadataMap` ↔ `tpt_proto_grpc::Metadata` (lowercase text
keys, `-bin` base64 binary values). Deadlines: the `grpc-timeout` header is
translated to a deadline/cancellation automatically; per-RPC cancellation
tokens are available through the call context.

Cross-cutting concerns map differently: instead of tower `Layer`s, tpt-proto
uses first-class hooks on the server builder —
`with_security(SecurityPolicy)` (bearer/metadata authenticators, role
authorizers), `with_observability(Observability)` (metrics, structured logs,
health checking), plus built-in keepalive, graceful shutdown/draining,
connection limits, TLS termination, and compression negotiation.

## Migration checklist

1. Replace runtime deps: drop `prost`/`tonic`, add `tpt-proto-core`
   (+ `tpt-proto-grpc` if RPCs).
2. Replace `[build-dependencies]`: `prost-build`/`tonic-build` →
   `tpt-proto-build`; switch `build.rs` to `compile_protos[_simple]`.
3. Delete hand-written derive-annotated message structs; generate from your
   original `.proto` files instead.
4. Rename imports at call sites: `prost::Message` → `tpt_proto_core::Message`
   (`encode_to_vec` now returns `Result`); `tonic::*` → `tpt_proto_grpc::*`.
5. Swap server/client construction as shown above; generated server traits use
   PascalCase names (`Greeter`, not `greeter`).
6. Re-add cross-cutting behavior with `SecurityPolicy`/`Observability` where you
   previously used tower layers/interceptors.
7. Run your existing roundtrip tests — wire compatibility means fixtures
   encoded by prost decode identically.

## Compatibility notes

- **Wire format is fully compatible**: payloads produced by prost/protobuf
  decode identically, including packed/unpacked repeated fields, maps, legacy
  groups, and unknown-field preservation.
- **JSON** follows canonical protobuf JSON rules (lowerCamelCase keys, base64
  bytes, stringified 64-bit ints, WKT special forms) with relaxed and
  default-emission options.
- **gRPC** speaks standard HTTP/2 gRPC (h2c and TLS): framing, trailers,
  `grpc-status`/`grpc-message`, deadlines, metadata, compression, streaming.

