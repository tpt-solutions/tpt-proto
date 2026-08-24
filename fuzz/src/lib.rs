//! Fuzz targets for the `tpt-proto` implementation of the Protocol Buffers
//! ecosystem.
//!
//! Each entry in [`targets`] exercises one decoder / front-end parser against
//! untrusted input. The functions are intentionally panic-tolerant at the
//! boundary (they discard `Result`s that are expected to be `Err` for invalid
//! input) so that a crash *inside* a decoder — not merely a rejected input —
//! is what a fuzzer reports as a finding.
//!
//! Two ways to run:
//!
//! 1. **libFuzzer (recommended, CI / Linux):** `cargo fuzz run <target>`. The
//!    `fuzz_targets/*.rs` files forward straight into these functions.
//! 2. **Portable corpus harness (any platform):** `cargo run` (or
//!    `cargo test`) builds the harness in `src/main.rs` + `tests/smoke.rs`,
//!    which feed a seeded corpus and a bounded amount of randomised input into
//!    every target, catching and reporting panics instead of aborting.

pub mod targets {
    use std::sync::{Arc, OnceLock};

    use base64::engine::general_purpose::STANDARD as B64;
    use base64::Engine as _;
    use tpt_proto_compiler::compile;
    use tpt_proto_core::{Message, Reader, UnknownFieldSet, Writer};
    use tpt_proto_descriptor::{DescriptorProto, FileDescriptorSet};
    use tpt_proto_grpc::codec;
    use tpt_proto_grpc::{Compression, Metadata, Status, MAX_METADATA_SIZE_DEFAULT};
    use tpt_proto_language::parse_file;
    use tpt_proto_reflect::{DescriptorPool, DynamicMessage};

    /// A small but non-trivial schema used by the schema-driven fuzz targets
    /// (JSON, text, dynamic decode). It exercises strings, ints, bytes,
    /// repeated fields, maps, oneofs, enums, and nested messages.
    const SCHEMA: &str = r#"
syntax = "proto3";
package fuzz;

enum Color { RED = 0; GREEN = 1; BLUE = 2; }

message Sub {
  int32 x = 1;
  string y = 2;
}

message Msg {
  string name = 1;
  int32 id = 2;
  repeated string tags = 3;
  map<string, int32> labels = 4;
  Color color = 5;
  Sub sub = 6;
  oneof contact {
    string email = 7;
    string phone = 8;
  }
  bytes blob = 9;
}
"#;

    struct Shared {
        pool: DescriptorPool,
        msg: Arc<DescriptorProto>,
    }

    fn shared() -> &'static Shared {
        static INSTANCE: OnceLock<Shared> = OnceLock::new();
        INSTANCE.get_or_init(|| {
            let parsed = parse_file("fuzz.proto", SCHEMA);
            assert!(
                !parsed.diagnostics.has_errors(),
                "fuzz schema failed to parse: {:?}",
                parsed.diagnostics
            );
            let (fd, diags) = compile(&parsed.file);
            assert!(
                !diags.has_errors(),
                "fuzz schema failed to compile: {:?}",
                diags
            );
            let pool = DescriptorPool::from_file(&fd);
            let msg = pool.lookup_message("fuzz.Msg").expect("fuzz.Msg missing");
            Shared { pool, msg }
        })
    }

    /// Decode arbitrary wire-format bytes with no schema, exercising the core
    /// [`Reader`] and [`UnknownFieldSet`] storage + re-emit path.
    pub fn binary_decoder(data: &[u8]) {
        let mut r = Reader::new(data);
        let mut set = UnknownFieldSet::new();
        while !r.is_empty() {
            match r.read_tag() {
                Ok(tag) => {
                    // Unknown-field storage enforces `max_unknown_bytes`; a
                    // malformed sub-structure yields Err and we stop.
                    if set.store(tag, &mut r).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        // Re-emit preserved unknowns; must round-trip without panicking.
        let mut w = Writer::new();
        set.encode(&mut w);
        let _ = w.into_vec();
    }

    /// Decode arbitrary bytes as a `FileDescriptorSet` (the descriptor binary
    /// format), then build a pool from it and re-encode. This fuzzes the
    /// descriptor model's `merge_from`/`decode` path deeply.
    pub fn descriptor_decoder(data: &[u8]) {
        if let Ok(set) = FileDescriptorSet::decode(data) {
            let pool = DescriptorPool::from_set(&set);
            // Touch the pool so indexing of (nested) messages/enums is exercised.
            let _ = pool.lookup_message("");
            // Re-encode; must succeed for anything we successfully decoded.
            let _ = set.encode_to_vec();
        }
    }

    /// Parse arbitrary text as a `.proto` source and run semantic analysis.
    /// This fuzzes the language lexer/parser and the compiler front-end.
    ///
    /// NOTE: the language parser is recursive; a pathological input can still
    /// exhaust the stack. That is a tracked hardening item (see
    /// `fuzz/README.md`).
    pub fn language_parser(data: &[u8]) {
        let src = String::from_utf8_lossy(data);
        let parsed = parse_file("fuzz.proto", &src);
        // Compile even if there are diagnostics; the compiler must not panic on
        // any source the parser accepted (or partially accepted).
        let _ = compile(&parsed.file);
    }

    /// Parse arbitrary text as protobuf JSON against the shared schema.
    pub fn json_decoder(data: &[u8]) {
        let json = match std::str::from_utf8(data) {
            Ok(s) => s,
            Err(_) => return,
        };
        let Shared { pool, msg } = shared();
        let _ = tpt_proto_json::json_string_to_message(
            pool,
            &msg,
            json,
            &tpt_proto_json::JsonOptions::default(),
        );
    }

    /// Parse arbitrary text as protobuf text format against the shared schema.
    pub fn text_parser(data: &[u8]) {
        let text = match std::str::from_utf8(data) {
            Ok(s) => s,
            Err(_) => return,
        };
        let Shared { pool, msg } = shared();
        let _ = tpt_proto_text::text_to_message(
            pool,
            &msg,
            text,
            &tpt_proto_text::TextOptions::default(),
        );
    }

    /// Decode arbitrary wire-format bytes as the shared `fuzz.Msg` message via
    /// the descriptor-driven [`DynamicMessage`], then re-encode. This is the
    /// deepest decoder target: it drives the reflection decoder (now
    /// depth-limited across nesting) and the dynamic encoder.
    pub fn dynamic_decoder(data: &[u8]) {
        let Shared { pool, msg } = shared();
        let mut r = Reader::new(data);
        if let Ok(dm) = DynamicMessage::decode(pool, msg.clone(), &mut r) {
            // Re-encode the decoded message; must not panic.
            let _ = dm.encode();
        }
    }

    /// gRPC message-frame decoding (`tpt-proto-grpc::codec`). A small fixed
    /// `max_message_size` targets the decompression-bomb bug class directly:
    /// a tiny gzip stream crafted to expand past the cap must be rejected
    /// cheaply (never fully materialised), and anything that *does* decode
    /// must re-encode under the same cap without panicking.
    pub fn codec_decode_message(data: &[u8]) {
        let Some((&flag, frame)) = data.split_first() else {
            return;
        };
        // The first byte doubles as the `grpc-encoding` algorithm selector;
        // the rest is exactly what an attacker would put on the wire.
        let encoding = match flag % 3 {
            0 => Compression::Identity,
            1 => Compression::Gzip,
            _ => Compression::Other("fuzz".into()),
        };
        const MAX_MESSAGE_SIZE: usize = 64 * 1024; // deliberately small fixed cap
        if let Ok(msg) = codec::decode_message(frame, encoding.clone(), MAX_MESSAGE_SIZE) {
            // Whatever decodes must re-encode under the same cap; no panic.
            let _ = codec::encode_message(&msg, encoding, MAX_MESSAGE_SIZE);
        }
    }

    /// gRPC metadata parsing. Arbitrary `(key, value)` pair sequences must be
    /// validated or rejected gracefully — never panic, never allocate without
    /// bound — in both header mode (reserved keys rejected) and trailer mode
    /// (reserved `grpc-*` keys allowed), plus the header rendering round-trip.
    pub fn grpc_metadata(data: &[u8]) {
        let text = String::from_utf8_lossy(data);
        let mut pairs: Vec<(String, String)> = Vec::new();
        let mut parts = text.split('\n');
        while let (Some(k), Some(v)) = (parts.next(), parts.next()) {
            pairs.push((k.trim().to_string(), v.trim().to_string()));
        }

        // Oversized/invalid sets are rejected with Err; a panic is a finding.
        let _ = Metadata::from_headers(&pairs, MAX_METADATA_SIZE_DEFAULT);
        let _ = Metadata::from_trailers(&pairs, MAX_METADATA_SIZE_DEFAULT);

        // Whatever parses must survive the render -> reparse path.
        if let Ok(m) = Metadata::from_trailers(&pairs, usize::MAX) {
            let rendered = m.to_headers();
            let _ = Metadata::from_headers(&rendered, MAX_METADATA_SIZE_DEFAULT);
        }
    }

    /// gRPC status/trailer parsing, including rich `google.rpc.Status` detail
    /// decoding. The whole input is additionally attached as a
    /// `grpc-status-details-bin` blob so the Any-carrying detail decoder runs
    /// over attacker-controlled bytes on every execution.
    pub fn grpc_status(data: &[u8]) {
        let text = String::from_utf8_lossy(data);
        let mut trailers: Vec<(String, String)> = Vec::new();
        let mut parts = text.split('\n');
        while let (Some(k), Some(v)) = (parts.next(), parts.next()) {
            trailers.push((k.trim().to_string(), v.trim().to_string()));
        }
        // Base64 keeps the `-bin` key convention happy for arbitrary bytes.
        trailers.push(("grpc-status-details-bin".to_string(), B64.encode(data)));

        // A modest cap bounds work while admitting the synthetic detail blob.
        if let Ok(m) = Metadata::from_trailers(&trailers, 2 * MAX_METADATA_SIZE_DEFAULT) {
            let status = Status::from_trailers(&m);
            let _ = format!("{status}"); // Display must not panic.

            // Re-serialize and re-parse; both directions must stay total.
            let re = status.to_trailers();
            if let Ok(m2) = Metadata::from_trailers(&re.to_headers(), usize::MAX) {
                let _ = Status::from_trailers(&m2);
            }
        }
    }
}

/// Portable harness helpers used by `src/main.rs` and `tests/smoke.rs`.
pub mod harness {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    use super::targets;

    /// Run every target against `data`. Panics propagate (used when a crash
    /// should be surfaced loudly).
    pub fn run_all(data: &[u8]) {
        targets::binary_decoder(data);
        targets::descriptor_decoder(data);
        targets::language_parser(data);
        targets::json_decoder(data);
        targets::text_parser(data);
        targets::dynamic_decoder(data);
        targets::codec_decode_message(data);
        targets::grpc_metadata(data);
        targets::grpc_status(data);
    }

    /// Run every target, returning the names of any that panicked. This is the
    /// "smoke" mode used by `cargo run` and `cargo test`, so a found crash is
    /// reported rather than aborting the whole process.
    pub fn run_all_catch(data: &[u8]) -> Vec<&'static str> {
        let mut crashed = Vec::new();
        for (name, f) in [
            ("binary_decoder", targets::binary_decoder as fn(&[u8])),
            ("descriptor_decoder", targets::descriptor_decoder as fn(&[u8])),
            ("language_parser", targets::language_parser as fn(&[u8])),
            ("json_decoder", targets::json_decoder as fn(&[u8])),
            ("text_parser", targets::text_parser as fn(&[u8])),
            ("dynamic_decoder", targets::dynamic_decoder as fn(&[u8])),
            ("codec_decode_message", targets::codec_decode_message as fn(&[u8])),
            ("grpc_metadata", targets::grpc_metadata as fn(&[u8])),
            ("grpc_status", targets::grpc_status as fn(&[u8])),
        ] {
            if catch_unwind(AssertUnwindSafe(|| f(data))).is_err() {
                crashed.push(name);
            }
        }
        crashed
    }
}
