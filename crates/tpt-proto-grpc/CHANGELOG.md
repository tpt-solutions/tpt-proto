# Changelog

All notable changes to `tpt-proto-grpc` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-08-24

### Added
- Initial release of the gRPC protocol, server, and client runtime
  (Phases 14 / 15).
- Transport-agnostic protocol types: `Metadata`, `Status`/`Code`,
  `RpcContext`, `Request`/`Response`, framing (`codec`, `framed`),
  `Compression`.
- Method model: `Method`, `MethodKind`, `Service`, `build_service`.
- Server: `Server`/`ServerBuilder`/`ServerConfig`, `ServiceHandler`,
  acceptors, `health` and `reflection` services.
- Client: `Client`/`Endpoint`, load balancing, `RetryPolicy`, `Channel`,
  `ClientStream`/`ServerStream`.
- HTTP/2 transport: `Transport` trait + `H2Transport`.
- Cross-cutting: `CancellationToken`, `timeout` parsing, `interceptor`s,
  `observability` (metrics/tracing/logging), and `security` (TLS, bearer/PEM
  auth, RBAC authorizers).
- `build_path` / `parse_path` helpers and `CONTENT_TYPE_GRPC*` constants.
- Optional `tls` feature for TLS client support.

[Unreleased]: https://github.com/tpt-solutions/tpt-proto/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/tpt-solutions/tpt-proto/releases/tag/v0.1.0
