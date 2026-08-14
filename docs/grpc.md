# gRPC Layer

`tpt-proto-grpc` implements the gRPC protocol on top of `tpt-proto` message
serialization (gRPC addendum §1–§20). It turns `.proto` service definitions
into a complete RPC system over HTTP/2.

> tpt-proto is an independent clean-room implementation. It is not an official
> Protocol Buffers implementation.

## Architecture

```text
.proto schema
  -> tpt-proto compiler/codegen
    -> generated message types
      -> tpt-proto-grpc service traits
        -> gRPC protocol layer
          -> HTTP/2 transport
```

The core compatibility surface is **gRPC over HTTP/2**. Other transports are out
of scope for the core layer (§20 of the addendum).

## Protocol

### HTTP method and path

Calls use HTTP/2 `POST` with the path:

```text
/package.ServiceName/MethodName
```

Example: `/example.UserService/GetUser`.

### Content type

`application/grpc` (with extended variants supported where required).

### Message framing

Each gRPC message is framed as:

```text
1 byte  compression flag
4 bytes big-endian message length
N bytes serialized protobuf message
```

The implementation supports compressed and uncompressed messages, enforces
maximum message size, protects against partial frames, and resets the stream on
malformed frames.

### Metadata

Metadata is key-value pairs. Keys are lowercase ASCII; binary values use the
base64 `-bin` suffix convention. Request headers, response headers, and
trailers are supported, with size limits and reserved-header protection.

### Trailers and status

gRPC status is communicated via trailers: `grpc-status`, `grpc-message`,
`grpc-status-details-bin`. The stack supports successful completion, error
completion, trailer-only responses, early errors, cancellation status, and
`DEADLINE_EXCEEDED`.

### Timeouts

The `grpc-timeout` header translates into a request deadline, cancellation
signal, and remaining-time API. Both client- and server-side expiry are
handled.

### Compression

Supports `identity`, `gzip`, and pluggable codecs, negotiated via
`grpc-encoding` / `grpc-accept-encoding`.

## Service model

All four method types are supported (§4):

| Type | Shape |
| --- | --- |
| Unary | one request → one response |
| Server streaming | one request → many responses |
| Client streaming | many requests → one response |
| Bidirectional streaming | many requests ↔ many responses |

Generated code (see [Code generation](codegen.md)) produces:

- async **server traits**, e.g. `async fn get_user(&self, Request<GetUserRequest>) -> Result<Response<User>, Status>`;
- strongly typed **client stubs**;
- streaming client/server sink APIs with `#[async_trait]`-style async-native
  ergonomics.

Every RPC context exposes: deadline, remaining time, cancellation token,
metadata, peer info, an extensions map, compression settings, and call-metrics
hooks.

## Error model

A structured `Status` type carries a **status code**, message, optional rich
**details** (compatible with `google.rpc.Status`), optional metadata, and
source context. Standard codes include `OK`, `CANCELLED`, `UNKNOWN`,
`INVALID_ARGUMENT`, `DEADLINE_EXCEEDED`, `NOT_FOUND`, `ALREADY_EXISTS`,
`PERMISSION_DENIED`, `RESOURCE_EXHAUSTED`, `FAILED_PRECONDITION`, `ABORTED`,
`OUT_OF_RANGE`, `UNIMPLEMENTED`, `INTERNAL`, `UNAVAILABLE`, `DATA_LOSS`,
`UNAUTHENTICATED`.

## Cancellation and deadlines

Cancellation is first-class: client cancellation terminates the request; server
handlers are notified on disconnect; deadlines propagate across async tasks;
downstream work is cancellable; cancelled streams release resources promptly;
and deadline expiry yields `DEADLINE_EXCEEDED`.

## Server and client runtime

### Server

Service registration and routing by service/method, concurrent-stream and
message/metadata size limits, keepalive, graceful shutdown, connection draining,
health checking, server reflection, TLS termination, opt-in cleartext HTTP/2
(h2c) for local dev, backpressure, request limits, and timeout enforcement.

### Client

Endpoint configuration, connection reuse/pooling, HTTP/2 multiplexing, retries
with policies and backoff, timeouts, deadlines, cancellation, metadata
injection, interceptors, load-balancing hooks, health checking, TLS,
compression, message-size limits, and streaming backpressure.

### Interceptors / middleware

A composable, type-safe middleware model can inspect or modify outgoing/incoming
requests and responses, plus metadata, status, deadline, cancellation, and
extensions. Typical uses: auth, authorization, logging, tracing, metrics, rate
limiting, request-ID propagation, error normalization.

## Observability

- **Metrics** — requests started/completed, duration, active streams,
  cancellations, deadline-exceeded, bytes and messages sent/received, connection
  errors, stream resets; labeled by `service`, `method`, `status code`, and
  `streaming type`.
- **Tracing** — spans with `rpc.system`, `rpc.service`, `rpc.method`,
  `rpc.grpc.status_code`.
- **Logging** — structured logs with request ID, service, method, status,
  deadline, peer, and cancellation reason.

## Health checking

Implements the gRPC health protocol with states `UNKNOWN`, `SERVING`,
`NOT_SERVING`, `SERVICE_UNKNOWN`, supporting overall and per-service health,
readiness/liveness integration, and dynamic updates.

## Server reflection

Clients can discover services, methods, message types, and descriptors — useful
for debugging and CLI tooling.

## Security

Production-ready security: TLS, ALPN negotiation for HTTP/2, mTLS, certificate
validation, token/metadata authentication, authorization hooks, peer-identity
inspection, request/metadata limits, and a safe cleartext opt-in for local
development. Documentation encourages secure defaults.

## Debugging tools

A `tpt-grpc` CLI supports health checks, reflection, service listing, unary
calls, and stream watching, with JSON/binary input, descriptor-based decoding,
and metadata/deadline/TLS/compression flags:

```sh
tpt-grpc health     localhost:50051
tpt-grpc reflect    localhost:50051
tpt-grpc list-services localhost:50051
tpt-grpc call       localhost:50051 example.UserService/GetUser
tpt-grpc watch-stream localhost:50051 example.UserService/WatchUsers
```

## Compatibility

`tpt-proto-grpc` is tested against real-world gRPC behavior, including unary,
all streaming modes, cancellation, deadlines, metadata, trailers, compression,
status codes, TLS, h2c, proxies, load balancers, HTTP/2 flow control, stream
reset behavior, and `GOAWAY` handling. Interop tests against reference
implementations are maintained as part of the test strategy (§18, §19 of the
addendum).

## Acceptance criteria

`tpt-proto-grpc` is complete when generated clients/servers compile, all call
modes and streaming work, deadlines/cancellation/metadata/trailers/status/rich
errors behave correctly, TLS and h2c work, health/reflection/interceptors/
observability work, interop tests pass, the CLI debugging tools work, security
limits are enforced, and documentation is complete (addendum §19, items 1–15).
