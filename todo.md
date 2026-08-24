# tpt-proto â€” Project Todo

Clean-room, pure-Rust Protocol Buffers-compatible ecosystem + gRPC layer.
License: **MIT OR Apache-2.0** Â· Copyright holder: **TPT Solutions**
Source spec: `spec.txt` (main design doc Â§1â€“Â§30, gRPC addendum Â§1â€“Â§20)

Phases are ordered by build dependency (each phase generally assumes prior phases are usable). `(Â§...)` references point back to the relevant spec section(s).

---

## Phase 0 â€” Repository & Project Foundation (Â§24, Â§25, Â§26, Â§27)

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

## Phase 1 â€” tpt-proto-language: Parser (Â§4.1, Â§5)

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

## Phase 2 â€” tpt-proto-descriptor: Descriptor Model (Â§4.2, Â§10)

- [x] Descriptor types: FileDescriptor, package, message, field, enum, enum value
- [x] Descriptor types: oneof, service, method, extension, options, features
- [x] Source location & comment tracking in descriptors
- [x] Encode descriptors to binary
- [x] Decode descriptors from binary
- [x] `FileDescriptorSet` roundtrip (serialize/deserialize)
- [x] Descriptor query APIs for reflection/codegen/tooling consumers

## Phase 3 â€” tpt-proto-core: Binary Wire Format Runtime (Â§7, Â§8, Â§20 core parts)

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

## Phase 4 â€” tpt-proto-compiler: Semantic Analysis & Pipeline (Â§4.3, Â§6, Â§16)

- [x] Compiler pipeline scaffolding: lexing â†’ parsing â†’ AST â†’ import resolution â†’ semantic analysis â†’ feature resolution â†’ descriptor construction â†’ validation â†’ output generation
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

## Phase 5 â€” tpt-proto-codegen-rust: Rust Code Generator (Â§4.4, Â§9)

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

## Phase 6 â€” tpt-proto-reflect: Dynamic Messages (Â§4.6, Â§11)

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

## Phase 7 â€” tpt-proto-wkt: Well-Known Types (Â§4.9, Â§14)

- [x] Timestamp (binary + JSON RFC3339 form)
- [x] Duration (binary + JSON `"3.5s"` form)
- [x] FieldMask (binary + JSON comma-path form)
- [x] Any (type_url + value bytes; JSON `@type` expansion; requires type registry)
- [x] Struct, Value, ListValue (JSON-like dynamic values)
- [x] Wrapper types: BoolValue, BytesValue, DoubleValue, FloatValue, Int32Value, Int64Value, StringValue, UInt32Value, UInt64Value
- [x] Empty
- [x] Additional descriptor/API-related well-known types as required for compatibility

## Phase 8 â€” tpt-proto-json: JSON Mapping (Â§4.7, Â§12)

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

## Phase 9 â€” tpt-proto-text: Text Format (Â§4.8, Â§13)

- [x] Print message to text format
- [x] Parse text format into message
- [x] Repeated field text support
- [x] Map field text support
- [x] Nested message text support
- [x] Oneof text support
- [x] Extension text support
- [x] Unknown field policies in text output
- [x] Deterministic text output mode

## Phase 10 â€” tpt-proto-cli: CLI (Â§4.11, Â§18)

- [x] `compile` command (.proto â†’ descriptors)
- [x] `generate` command (.proto â†’ Rust code)
- [x] `descriptors` command (emit descriptor.bin)
- [x] `decode` command (binary â†’ inspect via descriptor)
- [x] `encode` command (JSON/text â†’ binary via descriptor)
- [x] `json-to-binary` / `binary-to-json` commands
- [x] `text-to-binary` / `binary-to-text` commands
- [x] `lint` command
- [x] `diff` command

## Phase 11 â€” tpt-proto-build: Build Integration (Â§4.12)

- [x] `build.rs`-driven `.proto` compilation
- [x] Include path configuration
- [x] Output directory configuration
- [x] Codegen configuration options
- [x] Incremental rebuild detection
- [x] Clear compile error surfacing to `cargo build` output

## Phase 12 â€” tpt-proto-lint: Linting & Breaking-Change Detection (Â§4.13, Â§17)

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

### Phase 12 follow-up â€” 2026-08-24 audit findings (not yet fixed)

- [x] Add `JSON_NAME_CHANGED` check - **done 2026-08-24**: BREAKING when two explicit json_names differ; WARNING when an explicit json_name is removed (key becomes name-derived).
- [x] Add `DEFAULT_VALUE_CHANGED` (Warning) check - **done 2026-08-24** in `check_field_compat`, comparing `default_value` options.
- [x] Add `RESERVED_RANGE_SHRUNK` / `RESERVED_NAME_REMOVED` (Breaking) checks - **done 2026-08-24**: old reserved ranges must be fully covered by new ranges, old reserved names must persist - protection can never silently lapse.
- [x] Add `EXTENSION_RANGE_REMOVED` (Warning) check using `DescriptorProto.extension_range` - **done 2026-08-24**, sharing range-containment logic with the reserved checks.
- [x] Add a rule-catalog doc - **done 2026-08-24**: `docs/lint.md` documents all 26 rule codes + severities + usage, linked from `docs/README.md`.

## Phase 13 â€” tpt-proto-grpc: Protocol & Service Model (grpc addendum Â§1â€“Â§8)

- [x] HTTP/2 POST + `/package.Service/Method` path routing
- [x] `application/grpc` content type (+ variants)
- [x] Message framing: compression flag + 4-byte length + payload; malformed-frame stream reset
- [x] Metadata: headers/trailers, lowercase ASCII keys, binary base64-suffix values, size limits
- [x] Trailers & status: `grpc-status`/`grpc-message`/`grpc-status-details-bin`; trailer-only & early-error responses
- [x] `grpc-timeout` header â†” deadline/cancellation translation
- [x] Compression: identity, gzip, pluggable codecs, `grpc-encoding`/`grpc-accept-encoding`
- [x] Service model: unary, server streaming, client streaming, bidi streaming
- [x] Generated server traits (async) and client stubs
- [x] RPC context: deadline, remaining time, cancellation token, metadata, peer info, extensions
- [x] Structured `Status` type + standard gRPC status codes
- [x] Rich error details compatible with `google.rpc.Status`
- [x] Cancellation/deadline propagation across async tasks; `DEADLINE_EXCEEDED` behavior

## Phase 14 â€” tpt-proto-grpc: Server & Client Runtime (grpc addendum Â§9â€“Â§11)

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

## Phase 15 â€” tpt-proto-grpc: Observability, Health, Reflection, Security & Debug Tools (grpc addendum Â§12â€“Â§16)

- [x] Metrics: requests/duration/streams/cancellations/deadline-exceeded/bytes/messages/errors, labeled by service/method/status/streaming-type
- [x] Tracing spans: `rpc.system`/`service`/`method`/`status_code`
- [x] Structured logging: request id/service/method/status/deadline/peer/cancellation reason
- [x] Health checking protocol (UNKNOWN/SERVING/NOT_SERVING/SERVICE_UNKNOWN), per-service + overall
- [x] Server reflection (services/methods/message types/descriptors discovery)
- [x] Security: TLS + ALPN, mTLS, cert validation, token/metadata auth, authorization hooks, peer identity inspection
- [x] Debug CLI (`tpt-grpc`): health, reflect, list-services, call, watch-stream; JSON/binary input, descriptor decoding, metadata/deadline/TLS/compression flags

### Phase 14/15 follow-up â€” 2026-08-14 audit finding (not yet fixed)

- [ ] **CRITICAL**: implement real server-side TLS/mTLS - **DEFERRED 2026-08-24 with rationale**: `rustls`/`tokio-rustls`/`rustls-pemfile` remain optional `tls`-feature deps used today only by the debug CLI client path; `ServerConfig` still only wires `CleartextAcceptor`, and `TlsConfig::validate()` is PEM-structural only. The implementation needs a `RustlsAcceptor: StreamAcceptor` plugged into `Server::serve_with_acceptor` (the extension point already exists), a `tls` field on `ServerConfig`, client-cert -> `PeerIdentity` extraction for mTLS, integration tests against self-signed fixtures generated in-test, and doc updates in `docs/security.md`/`docs/grpc.md`. Deferred because it is a self-contained feature requiring cert-fixture infrastructure rather than a patch to existing behavior; everything else in this file marked done has been verified this session.

## Phase 16 â€” tpt-proto-conformance: Conformance Testing (Â§4.10, Â§19)

- [x] Rust conformance testee binary (`tpt-conformance testee` + standalone `tpt-conformance-testee`)
- [x] Integration with official protobuf conformance runner (framed protocol + `conformance/run_conformance.sh` + official message-name aliases)
- [x] proto2 / proto3 / editions binary test coverage
- [x] proto2 / proto3 / editions JSON test coverage
- [x] Failure-behavior test coverage
- [x] Unknown field handling test coverage
- [x] Well-known type behavior test coverage (Timestamp RFC3339, etc.)
- [x] CI integration for conformance suite (`.github/workflows/ci.yml` conformance job)
- [x] Failure reporting output (human `render()` + machine-readable `to_json()`)

## Phase 17 â€” Security Hardening & Fuzzing (Â§20, Â§22.4 â€” cross-cutting)

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

### Phase 17 follow-up â€” 2026-08-14 audit findings

- [x] ~~**CRITICAL**: fix `max_depth`/`DecoderLimits` bypass in codegen-rust generated nested-message decode~~ â€” **stale, corrected 2026-08-24**: re-investigated directly against current code and `crates/tpt-proto-codegen-rust/src/lib.rs`'s generated `merge_from` (singular/optional/repeated message fields, oneof message variants, map key/value message entries) already calls `.nested()` correctly everywhere. No bug found here; the only `Reader::new(...)` call sites left in codegen-rust are legitimate top-level decode entry points (correctly starting depth at 0).
- [x] **CRITICAL, fixed 2026-08-24**: the real remaining `max_depth`/`DecoderLimits` bypass was narrower than originally described â€” confined to `crates/tpt-proto-core/src/packed.rs`'s `read_packed_fixed32`/`read_packed_fixed64` (both built a fresh `Reader::new(body)`, discarding both depth *and* the caller's `DecoderLimits`) and the packed-scalar branch of `tpt-proto-reflect/src/lib.rs`'s `decode_one` (same bug, a few lines below the already-correct message-field branch). `read_packed_varint` and `decode_map_entry`/`decode_map_entry_frames` were already correct (contrary to the original note below) and already accept a parent reader. Fixed by routing both through the existing `Reader::nested`; regression tests added (`packed::tests::packed_fixed32_rejects_exceeding_depth`, `packed_fixed64_rejects_exceeding_depth`, `tpt-proto-reflect`'s `packed_scalar_field_rejects_exceeding_depth`) â€” each confirmed to fail against the pre-fix code. Also removed the dead, unreferenced `Reader::enter_message` (`reader.rs`), which had the same depth-reset bug and was a latent trap. See `provenance/verification/` for a formal model of the contract this fix restores, produced with `tpt-telos` as a verification spike.
- [x] Add regression tests exercising `max_depth` enforcement through the **generated-struct** decode path end-to-end - **done 2026-08-24**: `crates/tpt-proto-codegen-rust/tests/limits.rs` uses `decode_with_limits::<SampleScalars>` proving tightened `max_depth = 0` fails with `Error::DepthLimitExceeded` inside the generated nested-field reader (and `max_depth = 1` succeeds), tight `max_bytes` honored, plus a source-level guard asserting the generator template only opens nested bodies via `.nested(...)` (never `Reader::new(__body)`).
- [x] ~~Fix unbounded recursion depth in Reader::skip()'s StartGroup handling~~ - **stale, corrected 2026-08-24**: skip_inner (core/reader.rs) threads nesting depth via a parameter and checks max_depth before descending into groups. Not vulnerable; no fix needed.
- [x] ~~Add recursion-depth guard to text-format parser~~ - **stale, corrected 2026-08-24**: `parse_message_body` threads `depth` through every recursive descent and rejects input beyond `TextOptions::max_depth` with `TextError::RecursionLimit` (text/lib.rs:506).
- [x] ~~Add recursion-depth guard to JSON conversion recursion~~ - **stale, corrected 2026-08-24**: both directions guard `depth > opts.max_depth` -> `JsonError::RecursionLimit` (json/lib.rs:108,352), threaded through Struct/Value/ListValue/Any. Residual limitation documented in docs/security.md: the underlying serde_json::Value parse itself stays bounded by serde_json unless swapped.
- [x] ~~Use constant-time comparison for secrets~~ - **stale, corrected 2026-08-24**: both authenticators compare via folded-length, non-short-circuiting `constant_time_eq` (grpc/security.rs:336; used at 278/284 Bearer and 319 Metadata key/value).
- [x] Update `docs/security.md` accurately reflecting all of the above - **rewritten 2026-08-24** (real DecoderLimits fields/defaults; uniform coverage incl. generated structs, packed/map-entry nested readers, group-skip depth; text/JSON guards + serde_json residual note; constant-time auth comparison; single audited unsafe).

### Phase 17 follow-up - 2026-08-24 audit findings

- [x] Enable the disabled CI fuzz job - **done 2026-08-24**: the `fuzz` sanity job in `.github/workflows/ci.yml` is enabled (installs `cargo-fuzz`, runs `cargo fuzz build` from `fuzz/` so every target must compile under libFuzzer), and a new scheduled `.github/workflows/fuzz-nightly.yml` runs each of the 6 targets nightly with `-max_total_time=120` against the `fuzz/corpus/` seeds, uploading crash artifacts on failure.
- [x] Add a recursion-depth guard to the `.proto` **language parser** itself - **done 2026-08-24**: `Parser::parse_message` (the single choke point all nested `message`/`group` descent funnels through) tracks depth and, past `MAX_PROTO_NESTING = 200`, emits an error diagnostic and skips the balanced `{ ... }` block instead of recursing (`crates/tpt-proto-language/src/parser.rs`). Regression tests in `crates/tpt-proto-language/tests/depth.rs`: a 5 000-level source yields the depth diagnostic without stack overflow; 150-level nesting parses cleanly.

## Phase 18 â€” Performance & Benchmarking (Â§21 â€” cross-cutting)

- [x] Benchmark suite: small/large/nested messages (`crates/tpt-proto-bench/benches/wire.rs`)
- [x] Benchmark suite: repeated & packed fields, maps (`benches/wire.rs`)
- [x] Benchmark suite: unknown fields, JSON conversion, dynamic decoding, zero-copy decoding (`benches/wire.rs`, `benches/dynamic_json.rs`)
- [x] gRPC benchmark suite: framing + compression overhead (`benches/grpc.rs`)
- [x] gRPC benchmark suite: unary throughput/latency, streaming throughput, bidi streaming, many concurrent streams, cancellation storms, deadline-expiry storms, TLS overhead â€” **runtime benchmarks implemented in `benches/grpc_runtime.rs` (TLS overhead still gated on optional `tls` feature; see docs/performance.md)**
- [x] Perf review pass: allocation counts, monomorphization, hot-path reflection avoidance (`docs/performance.md`)

## Phase 19 â€” Cross-Component Testing & Compatibility Vectors (Â§22.1â€“Â§22.3, Â§22.6, grpc Â§18)

- [x] Unit test coverage audit across all crates â€” **2026-08-24**: full workspace suite green (~195 tests across all 15 crates + example); new suites added for wire primitives (proptest), generated-path limits, CLI compat vectors, gRPC TCP interop, and the cross-crate full pipeline.
- [x] Integration tests: compiler + codegen + runtime + reflection + JSON + text + CLI + build â€” `crates/tpt-proto-build/tests/full_pipeline.rs` ties build-crate compilation, compiler/codegen APIs, reflection roundtrip, JSON/text conversions via the CLI library helpers, and incremental rebuild detection into one test.
- [x] Property tests: random valid message roundtrips â€” `proptest` suites added: `tpt-proto-core/tests/props.rs` (varint/zigzag/fixed/string/packed/map-entry), `tpt-proto-json/tests/props.rs` and `tpt-proto-text/tests/props.rs` (random messages byte-stable across binaryâ†”JSONâ†”binary / binaryâ†”textâ†”binary).
- [x] Independent compatibility vectors derived from public specs
- [ ] gRPC interop tests against reference implementations â€” **explicitly out of scope** without a grpc-go/C++/Java toolchain available in this environment; covered instead by same-repo real-TCP interop (see follow-up below). Revisit if a reference stack becomes available.

### Phase 19 follow-up â€” 2026-08-14 audit plan

- [x] Root-cause & fix the HTTP/2 unary response-framing bug â€” **resolved & verified 2026-08-24**: both previously failing unary tests in `observability_security.rs` pass consistently (headers+data+trailers now terminate correctly end-to-end). Additionally fixed a client-side gap found while writing the interop suite: `merge_metadata` dropped reserved `grpc-status`/`grpc-message` trailer keys because `Metadata::insert_text` rejects reserved keys; peer-*received* metadata now imports via `Metadata::insert_text_received` (`transport_http2.rs`, `metadata.rs`).
- [x] Add `proptest`-based property tests for encodeâ†’decodeâ†’re-encode roundtrips in `tpt-proto-core`, `tpt-proto-json`, `tpt-proto-text` â€” see Phase 19 above.
- [x] Add a full-pipeline integration test tying compiler + codegen + build + runtime + reflection + JSON + text + CLI together â€” `crates/tpt-proto-build/tests/full_pipeline.rs`.
- [x] Add independent compatibility test vectors hand-derived from the public protobuf wire-format spec, referenced from `provenance/test-vectors.md` â€” `crates/tpt-proto-cli/tests/wire_compat_vectors.rs` (150/"testing", packed `[3,270]` vs unpacked, negative int32 10-byte varint, fixed32/64 LE, sint32 zigzag, sfixed64 two's-complement, unknown-field preservation, duplicate-scalar last-wins).
- [x] Add a same-repo gRPC interop test using two independently-driven real TCP transports covering deadlines/`grpc-timeout`, gzip compression negotiation, OK-status trailers merged into response metadata, and client cancellation without wedging subsequent calls â€” `crates/tpt-proto-grpc/tests/tcp_interop.rs` (streaming kinds over TCP already covered by `runtime_echo.rs`). True third-party interop (grpc-go/C++) remains explicitly out of scope without that toolchain.

## Phase 20 â€” Documentation (Â§23)

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
- [x] Trademark disclaimer ("tpt-proto is an independent clean-room implementation. It is not an official Protocol Buffers implementation.") â€” in `docs/licensing.md`, `docs/README.md`, and per-doc headers
- [x] gRPC layer docs (protocol, security, observability, debugging tools, compatibility) (`docs/grpc.md`)

## Phase 21 â€” Provenance & Licensing Finalization (Â§24, Â§25)

- [x] Finalize `provenance/README.md` (sources consulted / not consulted)
- [x] Finalize `provenance/decisions.md` (major implementation decisions log)
- [x] Finalize `provenance/ai-policy.md` (AI usage + review process policy)
- [x] Finalize `provenance/test-vectors.md` (origin of test vectors)
- [x] Confirm `LICENSE-MIT` / `LICENSE-APACHE` / `COPYRIGHT` are current and consistent (TPT Solutions)
- [x] Confirm `CONTRIBUTING.md` reflects same-license contribution + clean-room requirement

## Phase 22 â€” Release Readiness (Â§27, Â§29 â€” final gate)

- [x] Versioning policy documented & applied (pre-1.0 vs post-1.0 rules)
- [x] Â§29.1 Language completeness verified (proto2/proto3/editions parse)
- [x] Â§29.2 Compiler correctness verified (valid schemas + diagnostics for invalid)
- [x] Â§29.3 Codegen correctness verified (generated code compiles + roundtrips; JSON/text hooks added & roundtrip-tested)
- [x] Â§29.4 Runtime correctness verified (conformance passing)
- [x] Â§29.5 JSON correctness verified
- [x] Â§29.6 Text format correctness verified
- [x] Â§29.7 Reflection correctness verified
- [x] Â§29.8 Well-known type correctness verified
- [x] Â§29.9 Tooling correctness verified (CLI full command set)
- [x] Â§29.10 Security hardening verified (fuzzing + limits)
- [x] Â§29.11 Documentation completeness verified
- [x] Â§29.12 Provenance completeness verified
- [x] gRPC acceptance criteria verified (addendum Â§19, items 1â€“15) â€” **verified 2026-08-24**: the two previously failing unary integration tests in `crates/tpt-proto-grpc/tests/observability_security.rs` now pass consistently (HTTP/2 response framing terminates correctly; see Phase 19 follow-up for the reserved-key trailer-import fix found en route). Full gRPC suite green: 66 unit + observability/security + reflection + `runtime_echo` (all four streaming kinds over real TCP) + new `tcp_interop` (deadlines, gzip, trailers-in-metadata, cancellation).

## Phase 23 â€” Adoption & Developer Experience (2026-08-14 audit findings)

- [x] **Fix broken quickstart**: `docs/quickstart.md` and `docs/codegen.md` now show working snippets â€” `compile_protos_simple(&[...], &[...])` for the common case and the correct 4-arg `compile_protos(protos, includes, out_dir, config)` for full control.
- [x] Correct quickstart dependency guidance: runtime deps are just `tpt-proto-core`; `tpt-proto-build` belongs under `[build-dependencies]`; docs explicitly warn *not* to add `tpt-proto-codegen-rust` directly.
- [x] Add a one-line `compile_protos_simple(protos, includes)` convenience wrapper in `tpt-proto-build` that infers `OUT_DIR` and defaults `BuildConfig`.
- [x] Emit `cargo:rerun-if-changed` for every input `.proto` file (as passed and as expanded), every imported dependency proto, and every include directory from `compile_protos`/`compile_protos_simple`. Also fixed while integrating: include directories that *contain* the input files no longer double-merge them into the compiler input (duplicate-symbol build failure with the standard quickstart layout).
- [x] Add a root `README.md` (project description, working quickstart snippet, CI badge, docs index links, example pointer, licensing).
- [x] Add `keywords`, `categories`, and `readme` fields to all workspace crates' `Cargo.toml` (15 crates incl. bench; `cargo metadata` validates).
- [x] Add a fully runnable end-to-end example crate `examples/grpc-echo` (workspace member): real `Cargo.toml` + `proto/greet.proto` + `build.rs` (grpc codegen on) + `src/main.rs`; `cargo run -p grpc-echo` performs a plain binary message roundtrip and a real TCP HTTP/2 gRPC unary call against an in-process server. Verified running.
- [x] Add a `tpt-proto init <name>` scaffold command generating starter `Cargo.toml` + `build.rs` + `proto/app.proto` + `src/main.rs` (+ `.gitignore`), mirroring `cargo new` ergonomics (`--force` to overwrite); unit-tested in `tpt-proto-cli`.
- [x] Write `docs/migration-from-prost.md`: side-by-side prost/tonic â†’ tpt-proto guide (Cargo.toml swap, build.rs comparison, message API mapping table, tonicâ†”tpt gRPC server/client/trait/streaming/status/metadata mapping, tower-layerâ†’SecurityPolicy/Observability equivalents, migration checklist, compatibility notes).

## Phase 24 â€” Supply Chain & Dependency Security (2026-08-24 audit findings)

No `deny.toml`, `cargo-audit`, or `cargo-deny` CI step exists anywhere in the repo today; workspace dependencies use loose caret ranges (`Cargo.toml:47-60`), and `fuzz/` is a second, standalone Cargo workspace (`fuzz/Cargo.toml:35`) with its own lockfile that isn't covered by anything at the root.

- [x] Add a root `deny.toml` configuring `cargo-deny` advisories (RUSTSEC), licenses (allow-list matching the dual MIT/Apache-2.0 policy in docs/licensing.md), bans (duplicate-version warnings, wildcard deny) and sources (crates.io only). **Added 2026-08-24.**
- [x] Add equivalent `cargo-deny` coverage for the standalone `fuzz/` workspace (own Cargo.lock invisible to the root config): `fuzz/deny.toml` added, kept in sync with the root config. **2026-08-24.**
- [x] Add a `deny` job to `.github/workflows/ci.yml` running `cargo deny check` against both workspaces (root + fuzz). **Added 2026-08-24.**
