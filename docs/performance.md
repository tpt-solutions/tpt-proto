# Performance & Benchmarking (Phase 18)

This document covers the Phase 18 benchmark suite and the accompanying
performance-review pass. The benchmarks live in the `tpt-proto-bench` crate
(a workspace member) and use a small, dependency-free timing harness
(`harness = false` benches driven by [`std::hint::black_box`]).

## Running

```sh
cargo bench -p tpt-proto-bench          # all suites
cargo bench -p tpt-proto-bench --bench wire
cargo bench -p tpt-proto-bench --bench dynamic_json
cargo bench -p tpt-proto-bench --bench grpc
```

Each line reports `ns/it` (the primary, size-independent metric) and an
aggregate `MB/s` (meaningful for large payloads; inflated for tiny ones because
`bytes_per_iter` is small).

## Benchmark groups

| Group | What it measures |
| --- | --- |
| `wire/small` | Tiny message (manual low-level encode, zero-copy borrowed decode, and descriptor-driven `DynamicMessage` encode/decode/roundtrip). |
| `wire/large` | A `Big` message with 2,000-entry `repeated int64`/`string`/`bytes` fields. |
| `wire/nested` | A 100-deep `Node{ val; Node child; }` chain (depth/limit stress). |
| `wire/packed` | `repeated int32/int64/double` (packed) encode/decode. |
| `wire/maps` | `map<string,int32>` + `map<int32,string>` with 1,000 entries. |
| `wire/unknown` | Decode a message that carries an unrecognized field, then re-encode (preserve policy → byte-equal). |
| `dynamic/*` | Isolated `DynamicMessage` decode (no generated code). |
| `json/*` | `message_to_json` / `json_to_message` / roundtrip for a small and a large message. |
| `grpc/frame/*` | gRPC message framing (encode/decode/roundtrip) at 1 B … 512 KiB. |
| `grpc/frame/*_gzip` | Compression overhead (gzip encode/decode) vs identity. |
| `grpc/runtime/unary/*` | Real HTTP/2 unary throughput/latency (h2c loopback) at 64 B … 512 KiB. |
| `grpc/runtime/server_stream/*` | Server-streaming fan-out (10/100/1000 messages). |
| `grpc/runtime/client_stream/*` | Client-streaming aggregate (10/100/1000 messages). |
| `grpc/runtime/bidi/*` | Bidirectional streaming echo (10/100/1000 messages). |
| `grpc/runtime/concurrent/*` | Many concurrent unary RPCs multiplexed on one h2 connection (64/256/1024). |
| `grpc/runtime/cancel_storm` | Cancellation storm: drop RPC futures almost immediately. |
| `grpc/runtime/deadline_storm` | Deadline-expiry storm: tiny `grpc-timeout` ⇒ `DEADLINE_EXCEEDED`. |

The `grpc/runtime/*` workloads run a real in-tree HTTP/2 server and client over a
loopback socket and are exercised by the `grpc_runtime` bench target:

```sh
cargo bench -p tpt-proto-bench --bench grpc_runtime
```

Iteration counts can be scaled for a quick smoke run with the `TPT_BENCH_SCALE`
env var (e.g. `TPT_BENCH_SCALE=0.01`).

## Performance-review pass

### Allocation counts

- **Zero-copy decode wins decisively on the hot path.** For the small message,
  the borrowed `read_string()` decode (`wire/small/decode_borrowed(zerocopy)`,
  ~92 ns/it) is ~3× faster than the descriptor-driven `DynamicMessage` decode
  (~2,360 ns/it) because no `String`/collection is allocated per field. The low
  level borrowed reader returns `&str`/`&[u8]` that alias the input buffer.
- **`DynamicMessage` allocates per message and per field.** Each message holds a
  `BTreeMap<u32, Value>`, and each scalar/list/map value is heap-allocated
  (`String`, `Vec`, `Box`-like enums). This is correct and schema-flexible, but
  it is the dominant cost in the dynamic path and should be avoided in tight
  loops.
- **Maps are the most allocation-heavy** (`wire/maps` ~1.1–1.2 ms/it for 1,000
  entries): each entry is built as a synthetic nested message and boxed into a
  `Value::Map`. A generated/flat map representation would avoid the per-entry
  message allocation.

### Monomorphization

- The core wire runtime (`tpt-proto-core`) is written with concrete,
  non-generic scalar codecs (`encode_int32`, `read_fixed64`, hand-rolled varint
  loops). There is no blanket generic dispatch, so the optimizer produces tight,
  specialized code — confirmed by the low `ns/it` on the low-level path.

### Hot-path reflection avoidance

- For maximum throughput, **generated messages (Phase 5) should be used in hot
  paths instead of `DynamicMessage`.** The dynamic encode of the small message
  is ~9× slower than the equivalent manual low-level encode (2,435 ns vs 259 ns).
  The benchmark suite currently exercises the reflection path because the codegen
  output is not yet wired into a compiled crate; a future `codegen-vs-dynamic`
  comparison is recommended once a generated type can be built in-tree.

### gRPC framing & compression

- Framing overhead is small: identity encode/decode/roundtrip of a 64 KiB message
  is ~3.6 µs / 1.2 µs / 5.0 µs.
- **gzip is only worthwhile for large payloads.** At 1 B it inflates the frame
  26× and costs ~47 µs to encode; at 512 KiB it shrinks the frame to ~0.45% of
  the payload and the encode cost (~615 µs) is dominated by the bandwidth saving.
  The negotiation logic should prefer `identity` for small messages and gate
  gzip on a size threshold.

## Deferred items

The Phase 14 server/client runtime is now complete, and the gRPC runtime
benchmarks listed in the table above (unary throughput/latency, server/client/bidi
streaming, many concurrent streams, cancellation storms, deadline-expiry storms)
have been implemented in the `grpc_runtime` bench target. The only remaining item
is **TLS overhead (h2c vs TLS)**, which is gated on the optional `tls` feature
(`rustls`/`tokio-rustls`) and a TLS-terminating `StreamAcceptor`; it can be added
once TLS is enabled in a benchmark build. See `docs/grpc.md` for the protocol
surface these exercise.
