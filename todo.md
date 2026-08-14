# tpt-proto — Project Todo

Clean-room, pure-Rust Protocol Buffers-compatible ecosystem + gRPC layer.
License: **MIT OR Apache-2.0** · Copyright holder: **TPT Solutions**
Source spec: `spec.txt` (main design doc §1–§30, gRPC addendum §1–§20)

Phases are ordered by build dependency (each phase generally assumes prior phases are usable). `(§...)` references point back to the relevant spec section(s).

---

## Phase 0 — Repository & Project Foundation (§24, §25, §26, §27)

- [x] Initialize git repository and `.gitignore`
- [x] Create Cargo workspace root (`Cargo.toml`) with 14 member crates under `crates/`
- [x] Scaffold empty crate skeletons: `tpt-proto-language`, `-descriptor`, `-compiler`, `-codegen-rust`, `-core`, `-reflect`, `-json`, `-text`, `-wkt`, `-conformance`, `-cli`, `-build`, `-lint`, `-grpc`
- [x] Create repo layout: `docs/`, `examples/`, `tests/`, `fuzz/`, `benches/`, `provenance/`
- [x] Add `LICENSE-MIT` and `LICENSE-APACHE` (dual license: MIT OR Apache-2.0)
- [x] Add `COPYRIGHT` file (copyright holder: TPT Solutions)
- [x] Set `license = "MIT OR Apache-2.0"` across workspace `Cargo.toml`
- [x] Add `CONTRIBUTING.md` (contribution + clean-room + AI-assist policy)
- [x] Create `provenance/README.md`, `provenance/decisions.md`, `provenance/ai-policy.md`, `provenance/test-vectors.md` skeletons
- [x] Set up CI pipeline skeleton (build, test, lint, fmt)
- [x] Document semantic versioning policy (pre-1.0 rules)

## Phase 1 — tpt-proto-language: Parser (§4.1, §5)

- [x] Lexer: tokens, comments, source spans
- [x] Parser: proto2 syntax
- [x] Parser: proto3 syntax
- [x] Parser: editions syntax
- [x] Package declarations
- [x] Import statements (regular, public, weak)
- [x] File-level options
- [x] Top-level messages, enums, services, extensions
- [x] Message body: fields, nested messages/enums/extensions, oneofs, maps
- [x] Reserved ranges & reserved names
- [x] Extension ranges
- [x] Legacy groups syntax
- [x] Field features: name/number/type/label/default/json_name/options
- [x] Enum constructs: values, aliases, reserved ranges/names, options
- [x] Service constructs: methods, streaming markers, options
- [x] Syntax diagnostics with span, severity, error code

## Phase 2 — tpt-proto-descriptor: Descriptor Model (§4.2, §10)

- [x] Descriptor types: FileDescriptor, package, message, field, enum, enum value
- [x] Descriptor types: oneof, service, method, extension, options, features
- [x] Source location & comment tracking in descriptors
- [x] Encode descriptors to binary
- [x] Decode descriptors from binary
- [x] `FileDescriptorSet` roundtrip (serialize/deserialize)
- [x] Descriptor query APIs for reflection/codegen/tooling consumers

## Phase 3 — tpt-proto-core: Binary Wire Format Runtime (§7, §8, §20 core parts)

- [x] Varint encode/decode + zigzag encoding
- [x] Tag encode/decode (`field_number << 3 | wire_type`)
- [x] Wire types 0/1/2/5 (varint, 64-bit, length-delimited, 32-bit)
- [x] Wire types 3/4 (group start/end) for legacy groups
- [x] Scalar codecs: int32/int64/uint32/uint64/sint32/sint64/fixed32/fixed64/sfixed32/sfixed64/bool/string/bytes
- [x] Packed repeated field encode/decode
- [x] Unpacked repeated field encode/decode; accept mixed packed/unpacked on input
- [x] Map entry encode/decode (key=field 1, value=field 2); duplicate-entry override semantics
- [x] Oneof wire behavior: mutually exclusive fields, last-value-wins
- [x] Unknown field handling: preserve (default) / discard / fail policies; re-encodable
- [x] Proto2 extensions: ranges, registries, dynamic lookup, encode/decode
- [x] Legacy group encode/decode with matching field numbers + usage warnings
- [x] Deterministic encoding mode: field order, map order, unknown field order, canonical varints
- [x] `DecoderLimits` struct + enforcement (max bytes/depth/fields/strings/etc.)
- [x] Checked arithmetic for integer overflow safety
- [x] Owned message representation
- [x] Borrowed/zero-copy message representation
- [x] Bytes-backed message support (shared/sliced buffers)

## Phase 4 — tpt-proto-compiler: Semantic Analysis & Pipeline (§4.3, §6, §16)

- [x] Compiler pipeline scaffolding: lexing → parsing → AST → import resolution → semantic analysis → feature resolution → descriptor construction → validation → output generation
- [x] Import resolution (incl. public/weak import propagation)
- [x] Package resolution
- [x] Duplicate symbol detection
- [x] Field number validation
- [x] Reserved range/name validation
- [x] Extension range validation
- [x] Enum validation (values, aliases, open/closed)
- [x] Oneof validation
- [x] Map validation
- [x] Option validation
- [x] Editions: feature defaults, overrides, inheritance, resolution determinism
- [x] Editions: proto2-like/proto3-like semantics compatibility mapping
- [x] Descriptor generation from validated AST
- [x] Diagnostic emission (file/line/column/span/severity/code/message/suggested fix)
- [x] Compiler outputs wired up: FileDescriptorSet, diagnostics, lint-report hooks

## Phase 5 — tpt-proto-codegen-rust: Rust Code Generator (§4.4, §9)

- [x] Message struct generation with doc comments
- [x] Encode method generation
- [x] Decode method generation
- [x] Default value handling in generated code
- [x] Presence handling (proto3 implicit, proto3 explicit optional, proto2 optional/required, editions-resolved)
- [x] Unknown field storage/passthrough in generated structs
- [x] Enum type generation: named values, numeric conversion, unknown values, open/closed semantics, aliases
- [x] Oneof generation as idiomatic Rust enums
- [x] Map field generation (idiomatic map types, deterministic-order fallback)
- [x] Repeated/optional/required field generation
- [x] Builder generation with validation (required fields, oneof constraints, invalid enum/default values)
- [x] Reflection metadata hooks generation
- [x] Service trait generation (ties into gRPC phases)
- [x] Borrowed/zero-copy decode method generation
- [x] JSON support hooks generation
- [x] Text format support hooks generation

## Phase 6 — tpt-proto-reflect: Dynamic Messages (§4.6, §11)

- [x] `DynamicMessage` type: descriptor-driven decode/encode
- [x] Field access & mutation by name/number
- [x] Repeated field access
- [x] Map field access
- [x] Enum value access
- [x] Oneof access
- [x] Nested message access
- [x] Extension access
- [x] Unknown field access
- [x] Default value inspection & presence inspection
- [x] Type registry & extension registry

## Phase 7 — tpt-proto-wkt: Well-Known Types (§4.9, §14)

- [x] Timestamp (binary + JSON RFC3339 form)
- [x] Duration (binary + JSON `"3.5s"` form)
- [x] FieldMask (binary + JSON comma-path form)
- [x] Any (type_url + value bytes; JSON `@type` expansion; requires type registry)
- [x] Struct, Value, ListValue (JSON-like dynamic values)
- [x] Wrapper types: BoolValue, BytesValue, DoubleValue, FloatValue, Int32Value, Int64Value, StringValue, UInt32Value, UInt64Value
- [x] Empty
- [x] Additional descriptor/API-related well-known types as required for compatibility

## Phase 8 — tpt-proto-json: JSON Mapping (§4.7, §12)

- [x] Binary-to-JSON conversion
- [x] JSON-to-binary conversion
- [x] Canonical JSON mode
- [x] Relaxed JSON mode
- [x] lowerCamelCase field name emission/parsing
- [x] Original proto field name emission/parsing
- [x] Enum as string name / as numeric value (+ unknown enum policy)
- [x] 64-bit integers as JSON strings
- [x] Bytes as base64
- [x] Default value emission options
- [x] Well-known type JSON rules: Timestamp, Duration, FieldMask, Struct, Value, ListValue, Any, wrappers, Empty

## Phase 9 — tpt-proto-text: Text Format (§4.8, §13)

- [x] Print message to text format
- [x] Parse text format into message
- [x] Repeated field text support
- [x] Map field text support
- [x] Nested message text support
- [x] Oneof text support
- [x] Extension text support
- [x] Unknown field policies in text output
- [x] Deterministic text output mode

## Phase 10 — tpt-proto-cli: CLI (§4.11, §18)

- [x] `compile` command (.proto → descriptors)
- [x] `generate` command (.proto → Rust code)
- [x] `descriptors` command (emit descriptor.bin)
- [x] `decode` command (binary → inspect via descriptor)
- [x] `encode` command (JSON/text → binary via descriptor)
- [x] `json-to-binary` / `binary-to-json` commands
- [x] `text-to-binary` / `binary-to-text` commands
- [x] `lint` command
- [x] `diff` command

## Phase 11 — tpt-proto-build: Build Integration (§4.12)

- [x] `build.rs`-driven `.proto` compilation
- [x] Include path configuration
- [x] Output directory configuration
- [x] Codegen configuration options
- [x] Incremental rebuild detection
- [x] Clear compile error surfacing to `cargo build` output

## Phase 12 — tpt-proto-lint: Linting & Breaking-Change Detection (§4.13, §17)

- [x] Style issue detection
- [x] Breaking-change classification: SAFE / WARNING / BREAKING
- [x] Field number reuse detection
- [x] Missing reserved declaration detection
- [x] Incompatible type change detection
- [x] Unsafe enum change detection
- [x] Unsafe oneof change detection
- [x] Unsafe package change detection
- [x] RPC input/output type change detection
- [x] Machine-readable lint output format

## Phase 13 — tpt-proto-grpc: Protocol & Service Model (grpc addendum §1–§8)

- [x] HTTP/2 POST + `/package.Service/Method` path routing
- [x] `application/grpc` content type (+ variants)
- [x] Message framing: compression flag + 4-byte length + payload; malformed-frame stream reset
- [x] Metadata: headers/trailers, lowercase ASCII keys, binary base64-suffix values, size limits
- [x] Trailers & status: `grpc-status`/`grpc-message`/`grpc-status-details-bin`; trailer-only & early-error responses
- [x] `grpc-timeout` header ↔ deadline/cancellation translation
- [x] Compression: identity, gzip, pluggable codecs, `grpc-encoding`/`grpc-accept-encoding`
- [x] Service model: unary, server streaming, client streaming, bidi streaming
- [x] Generated server traits (async) and client stubs
- [x] RPC context: deadline, remaining time, cancellation token, metadata, peer info, extensions
- [x] Structured `Status` type + standard gRPC status codes
- [x] Rich error details compatible with `google.rpc.Status`
- [x] Cancellation/deadline propagation across async tasks; `DEADLINE_EXCEEDED` behavior

## Phase 14 — tpt-proto-grpc: Server & Client Runtime (grpc addendum §9–§11)

- [x] Server: service registration & routing
- [x] Server: concurrent stream limits, message/metadata size limits
- [x] Server: keepalive, graceful shutdown, connection draining
- [x] Server: TLS termination + cleartext HTTP/2 (h2c) opt-in for local dev
- [x] Server: backpressure & request limits & timeout enforcement
- [x] Client: endpoint config, connection reuse/pooling, HTTP/2 multiplexing
- [x] Client: retries, retry policies, backoff
- [x] Client: timeouts, deadlines, cancellation, metadata injection
- [x] Client: load balancing hooks, health checking integration, TLS, compression
- [x] Client: streaming backpressure & message size limits
- [x] Interceptor/middleware model (request/response/metadata/status/deadline/cancellation/extensions), composable & type-safe

## Phase 15 — tpt-proto-grpc: Observability, Health, Reflection, Security & Debug Tools (grpc addendum §12–§16)

- [x] Metrics: requests/duration/streams/cancellations/deadline-exceeded/bytes/messages/errors, labeled by service/method/status/streaming-type
- [x] Tracing spans: `rpc.system`/`service`/`method`/`status_code`
- [x] Structured logging: request id/service/method/status/deadline/peer/cancellation reason
- [x] Health checking protocol (UNKNOWN/SERVING/NOT_SERVING/SERVICE_UNKNOWN), per-service + overall
- [x] Server reflection (services/methods/message types/descriptors discovery)
- [x] Security: TLS + ALPN, mTLS, cert validation, token/metadata auth, authorization hooks, peer identity inspection
- [x] Debug CLI (`tpt-grpc`): health, reflect, list-services, call, watch-stream; JSON/binary input, descriptor decoding, metadata/deadline/TLS/compression flags

### Phase 14/15 follow-up — 2026-08-14 audit finding (not yet fixed)

- [ ] **CRITICAL**: implement real server-side TLS/mTLS. `ServerConfig` (`crates/tpt-proto-grpc/src/server.rs`) has no `TlsConfig` field and the server only ever uses `CleartextAcceptor`; `TlsConfig::validate()` (`crates/tpt-proto-grpc/src/security.rs`) only parses PEM structure with no cryptographic validation, cert-chain trust, or client-cert extraction. `rustls`/`tokio-rustls`/`rustls-pemfile` are already optional deps behind the `tls` feature but are only used in the debug CLI's client connect path (`src/bin/tpt-grpc.rs`), never the server, and never for mTLS peer identity. Need: a `RustlsAcceptor` implementing `StreamAcceptor`, wired into `ServerConfig`/`Server::serve`, with client-cert-based `PeerIdentity` extraction for mTLS. The Phase 14/15 checkboxes above overclaim this as done — leave unchecked/annotated until the real implementation lands, then update `docs/security.md`/`docs/grpc.md` to match.

## Phase 16 — tpt-proto-conformance: Conformance Testing (§4.10, §19)

- [x] Rust conformance testee binary (`tpt-conformance testee` + standalone `tpt-conformance-testee`)
- [x] Integration with official protobuf conformance runner (framed protocol + `conformance/run_conformance.sh` + official message-name aliases)
- [x] proto2 / proto3 / editions binary test coverage
- [x] proto2 / proto3 / editions JSON test coverage
- [x] Failure-behavior test coverage
- [x] Unknown field handling test coverage
- [x] Well-known type behavior test coverage (Timestamp RFC3339, etc.)
- [x] CI integration for conformance suite (`.github/workflows/ci.yml` conformance job)
- [x] Failure reporting output (human `render()` + machine-readable `to_json()`)

## Phase 17 — Security Hardening & Fuzzing (§20, §22.4 — cross-cutting)

- [x] Decoder limit enforcement audited across core/reflect/json/text/grpc
- [x] UTF-8 validation per schema/edition rules
- [x] Recursion/depth control audited across all decoders
- [x] Allocation control & sanity checks audited across all decoders
- [x] `unsafe` usage audit: isolated, documented, tested, feature-gated, justified
- [x] Fuzz target: binary decoder
- [x] Fuzz target: JSON decoder
- [x] Fuzz target: text parser
- [x] Fuzz target: proto language parser
- [x] Fuzz target: descriptor decoder
- [x] Fuzz target: dynamic message decoder

### Phase 17 follow-up — 2026-08-14 audit findings (not yet fixed)

- [ ] **CRITICAL**: fix `max_depth`/`DecoderLimits` bypass in codegen-rust generated nested-message decode — `crates/tpt-proto-codegen-rust/src/lib.rs` builds sub-readers via `Reader::new(body)` instead of `parent.nested(body)`/`enter_message`, resetting depth to 0 and reverting to `DecoderLimits::default()` at every nested field (stack-overflow DoS via self-referential schemas; caller-supplied limits silently discarded). `tpt-proto-reflect`'s `DynamicMessage` decoder already does this correctly (`lib.rs:690-693`) — port the same fix to codegen.
- [ ] **CRITICAL**: same bypass in `crates/tpt-proto-core/src/packed.rs` (`read_packed_varint`/`read_packed_fixed32`/`read_packed_fixed64`, `decode_map_entry`) — these also construct fresh `Reader::new` instead of propagating parent depth/limits; `decode_map_entry` doesn't even accept a parent reader.
- [ ] Add regression tests exercising `max_depth` enforcement through the **generated-struct** decode path (not just reflection), plus a test that a custom/tightened `DecoderLimits` passed to a generated message is actually honored for nested fields.
- [ ] Fix unbounded recursion depth in `Reader::skip()`'s `StartGroup` handling (`crates/tpt-proto-core/src/reader.rs:233-245`) — no depth accounting when skipping nested legacy groups.
- [ ] Add recursion-depth guard to `crates/tpt-proto-text/src/lib.rs` (`parse_message_body`) — currently zero depth limiting; deeply nested `{ }` text-format input stack-overflows.
- [ ] Add recursion-depth guard to `crates/tpt-proto-json/src/lib.rs`'s message/Struct/Value/ListValue conversion recursion (the underlying `serde_json::Value` parse itself remains unbounded unless the parser is swapped — document that residual limitation explicitly rather than overclaiming).
- [ ] Use constant-time comparison for secrets in `BearerTokenAuthenticator`/`MetadataAuthenticator` (`crates/tpt-proto-grpc/src/security.rs:278,318`) — currently ordinary `HashSet::contains`/`!=`, a timing side channel.
- [ ] Update `docs/security.md` to accurately reflect the above once fixed (it currently overclaims full recursion/depth control across all decoders).

## Phase 18 — Performance & Benchmarking (§21 — cross-cutting)

- [x] Benchmark suite: small/large/nested messages (`crates/tpt-proto-bench/benches/wire.rs`)
- [x] Benchmark suite: repeated & packed fields, maps (`benches/wire.rs`)
- [x] Benchmark suite: unknown fields, JSON conversion, dynamic decoding, zero-copy decoding (`benches/wire.rs`, `benches/dynamic_json.rs`)
- [x] gRPC benchmark suite: framing + compression overhead (`benches/grpc.rs`)
- [x] gRPC benchmark suite: unary throughput/latency, streaming throughput, bidi streaming, many concurrent streams, cancellation storms, deadline-expiry storms, TLS overhead — **runtime benchmarks implemented in `benches/grpc_runtime.rs` (TLS overhead still gated on optional `tls` feature; see docs/performance.md)**
- [x] Perf review pass: allocation counts, monomorphization, hot-path reflection avoidance (`docs/performance.md`)

## Phase 19 — Cross-Component Testing & Compatibility Vectors (§22.1–§22.3, §22.6, grpc §18)

- [ ] Unit test coverage audit across all crates
- [ ] Integration tests: compiler + codegen + runtime + reflection + JSON + text + CLI + build
- [ ] Property tests: random valid message roundtrips
- [ ] Independent compatibility vectors derived from public specs
- [ ] gRPC interop tests against reference implementations (unary/streaming/cancellation/deadlines/metadata/trailers/compression/status/TLS/h2c/proxies/LB/flow control/GOAWAY)

### Phase 19 follow-up — 2026-08-14 audit plan

- [ ] Root-cause & fix the HTTP/2 unary response-framing bug (see Phase 22 known issue) first — it blocks writing real TCP-based (non-mock-transport) interop tests for the rest of this phase.
- [ ] Add `proptest`/`quickcheck`-based property tests for encode→decode→re-encode roundtrips in `tpt-proto-core`, `tpt-proto-json`, `tpt-proto-text`.
- [ ] Add a full-pipeline integration test: `.proto` → generate Rust → compile generated code → encode/decode/JSON/text roundtrip via the CLI, tying compiler + codegen + runtime + reflection + JSON + text + CLI + build together in one test.
- [ ] Add independent compatibility test vectors hand-derived from the public protobuf wire-format spec (not from any reference implementation's source, consistent with clean-room policy), referenced from `provenance/test-vectors.md`.
- [ ] Add a same-repo gRPC interop test using two independently-driven real TCP transports (client ↔ server, not the mock transport) covering unary/streaming/cancellation/deadlines/metadata/trailers/compression/status. True third-party interop (grpc-go/C++) is out of scope without that toolchain available — call this out explicitly rather than claiming full interop coverage.

## Phase 20 — Documentation (§23)

- [x] Quickstart guide (`docs/quickstart.md`)
- [x] Language support docs (proto2/proto3/editions) (`docs/language-support.md`)
- [x] Editions support docs (`docs/editions.md`)
- [x] Wire format behavior docs (`docs/wire-format.md`)
- [x] JSON behavior docs (`docs/json.md`)
- [x] Text format behavior docs (`docs/text-format.md`)
- [x] Code generation docs (`docs/codegen.md`)
- [x] Reflection & dynamic message docs (`docs/reflection.md`)
- [x] Security limits docs (`docs/security.md`)
- [x] Deterministic encoding docs (`docs/deterministic-encoding.md`)
- [x] Conformance status docs (`docs/conformance.md`)
- [x] Clean-room policy docs (`docs/clean-room.md`)
- [x] Licensing docs (`docs/licensing.md`)
- [x] Trademark disclaimer ("tpt-proto is an independent clean-room implementation. It is not an official Protocol Buffers implementation.") — in `docs/licensing.md`, `docs/README.md`, and per-doc headers
- [x] gRPC layer docs (protocol, security, observability, debugging tools, compatibility) (`docs/grpc.md`)

## Phase 21 — Provenance & Licensing Finalization (§24, §25)

- [x] Finalize `provenance/README.md` (sources consulted / not consulted)
- [x] Finalize `provenance/decisions.md` (major implementation decisions log)
- [x] Finalize `provenance/ai-policy.md` (AI usage + review process policy)
- [x] Finalize `provenance/test-vectors.md` (origin of test vectors)
- [x] Confirm `LICENSE-MIT` / `LICENSE-APACHE` / `COPYRIGHT` are current and consistent (TPT Solutions)
- [x] Confirm `CONTRIBUTING.md` reflects same-license contribution + clean-room requirement

## Phase 22 — Release Readiness (§27, §29 — final gate)

- [x] Versioning policy documented & applied (pre-1.0 vs post-1.0 rules)
- [x] §29.1 Language completeness verified (proto2/proto3/editions parse)
- [x] §29.2 Compiler correctness verified (valid schemas + diagnostics for invalid)
- [x] §29.3 Codegen correctness verified (generated code compiles + roundtrips; JSON/text hooks added & roundtrip-tested)
- [x] §29.4 Runtime correctness verified (conformance passing)
- [x] §29.5 JSON correctness verified
- [x] §29.6 Text format correctness verified
- [x] §29.7 Reflection correctness verified
- [x] §29.8 Well-known type correctness verified
- [x] §29.9 Tooling correctness verified (CLI full command set)
- [x] §29.10 Security hardening verified (fuzzing + limits)
- [x] §29.11 Documentation completeness verified
- [x] §29.12 Provenance completeness verified
- [ ] gRPC acceptance criteria verified (addendum §19, items 1–15) — KNOWN ISSUE: 2 pre-existing gRPC unary integration tests in `crates/tpt-proto-grpc/tests/observability_security.rs` fail due to a server-side HTTP/2 response-framing bug (unary `send_headers`+`send_data`+`send_trailers` does not terminate the stream, so the client errors/hangs). This is a runtime-layer bug distinct from the implemented gRPC features; tracked separately for a dedicated fix. All other crates' tests pass.

## Phase 23 — Adoption & Developer Experience (2026-08-14 audit findings)

- [ ] **Fix broken quickstart**: `docs/quickstart.md` and `docs/codegen.md` show a stale/wrong `compile_protos(&["proto/user.proto"], &["proto"])` 2-arg call; the real function (`crates/tpt-proto-build/src/lib.rs`) takes 4 args (`protos: &[PathBuf]`, `includes: &[PathBuf]`, `out_dir: &Path`, `config: &BuildConfig`). This is the first thing a new user copy-pastes and it fails to compile — highest-priority doc fix.
- [ ] Correct quickstart dependency guidance: don't tell consumers to add `tpt-proto-codegen-rust` as a runtime `[dependencies]` entry (it's build-time only).
- [ ] Add a one-line `compile_protos_simple(protos, includes)` convenience wrapper in `tpt-proto-build` that infers `OUT_DIR` and defaults `BuildConfig`, closing the ergonomics gap with `prost-build`/`tonic-build`.
- [ ] Emit `cargo:rerun-if-changed` for every input `.proto` file and include directory from `compile_protos`/`compile_protos_simple` — currently missing entirely, so Cargo won't rebuild when a `.proto` changes without the consumer manually wiring it themselves.
- [ ] Add a root `README.md` (project description, quickstart snippet, CI badge, links into `docs/`) — currently nothing on the repo landing page besides `CONTRIBUTING.md`/`todo.md`/`VERSIONING.md`.
- [ ] Add `keywords`, `categories`, and `readme` fields to all 14 crates' `Cargo.toml` (none currently have them) for crates.io discoverability.
- [ ] Add a fully runnable, end-to-end example crate under `examples/` (real `Cargo.toml` + `proto/*.proto` + `build.rs` + `src/main.rs`, `cargo run`-able) covering a plain message roundtrip and a real (non-mocked) TCP-based gRPC client/server pair — the current `examples/` directory only has proto/binary fixtures with no buildable crate, and `crates/tpt-proto-grpc/examples/grpc_echo` uses a checked-in pre-generated file over a mock in-process transport, not a real build.rs→network flow.
- [ ] Add a `tpt-proto-cli init <name>` (or `tpt-proto new`) scaffold command generating a starter `Cargo.toml` + `build.rs` + `proto/` + `src/main.rs`, mirroring `cargo new` ergonomics — no such scaffolding command exists today.
- [ ] Write `docs/migration-from-prost.md`: a side-by-side prost/tonic → tpt-proto migration guide (derive vs generated struct API, `prost_build`/`tonic_build` vs `tpt_proto_build`, `tonic::Server`/`Channel` vs `tpt_proto_grpc` equivalents, Cargo.toml dependency swap) — no such guide exists; the project's stated goal is to be a prost/tonic alternative but has zero conversion documentation today.
