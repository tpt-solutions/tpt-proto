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

    use tpt_proto_compiler::compile;
    use tpt_proto_core::{Message, Reader, UnknownFieldSet, Writer};
    use tpt_proto_descriptor::{DescriptorProto, FileDescriptorSet};
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
        ] {
            if catch_unwind(AssertUnwindSafe(|| f(data))).is_err() {
                crashed.push(name);
            }
        }
        crashed
    }
}
