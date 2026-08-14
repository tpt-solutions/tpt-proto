//! Conformance and test-message schemas (clean-room, §4.10, §19).
//!
//! Defines the `conformance.ConformanceRequest` / `ConformanceResponse` wire
//! schema plus a comprehensive `TestAllTypes` message per dialect
//! (proto2 / proto3 / editions) and the `google.protobuf.*` well-known types.
//! Everything is compiled with the project's own language parser + compiler so
//! the conformance testee exercises the full tpt-proto stack end-to-end.

use std::collections::HashMap;
use std::sync::Arc;

use tpt_proto_compiler::compile_set;
use tpt_proto_descriptor::{DescriptorProto, FileDescriptorSet};
use tpt_proto_language::parse_file;
use tpt_proto_reflect::DescriptorPool;

/// Wire schema for the conformance protocol (matches the official field
/// numbers so the testee interoperates with `conformance_test_runner`).
const CONFORMANCE_PROTO: &str = r#"
syntax = "proto3";
package conformance;

enum WireFormat {
  UNSPECIFIED = 0;
  PROTOBUF = 1;
  JSON = 2;
  JSPB = 3;
  TEXT_FORMAT = 4;
}

enum TestCategory {
  UNSPECIFIED_TEST = 0;
  BINARY_TEST = 1;
  JSON_TEST = 2;
  JSON_IGNORE_UNKNOWN_PARSING_TEST = 3;
  JSPB_TEST = 4;
  TEXT_FORMAT_TEST = 5;
}

message ConformanceRequest {
  oneof payload {
    bytes protobuf_payload = 3;
    string json_payload = 4;
    string text_payload = 7;
    string jspb_payload = 8;
  }
  WireFormat requested_output_format = 5;
  string message_type = 2;
  TestCategory test_category = 1;
}

message ConformanceResponse {
  oneof result {
    string parse_error = 1;
    string serialize_error = 2;
    string runtime_error = 3;
    bytes protobuf_payload = 4;
    string json_payload = 5;
    string text_payload = 7;
    string timeout_error = 8;
    bool skipped = 6;
  }
}
"#;

/// The `google.protobuf.*` well-known types used by the test messages. Field
/// numbers match the canonical definitions so the JSON special forms trigger.
const WKT_PROTO: &str = r#"
syntax = "proto3";
package google.protobuf;

message Timestamp { int64 seconds = 1; int32 nanos = 2; }
message Duration { int64 seconds = 1; int32 nanos = 2; }
message FieldMask { repeated string paths = 1; }
message Empty {}
message Struct { map<string, Value> fields = 1; }
message Value {
  oneof kind {
    int32 null_value = 1;
    double number_value = 2;
    string string_value = 3;
    bool bool_value = 4;
    Struct struct_value = 5;
    ListValue list_value = 6;
  }
}
message ListValue { repeated Value values = 1; }
message DoubleValue { double value = 1; }
message FloatValue { float value = 1; }
message Int64Value { int64 value = 1; }
message UInt64Value { uint64 value = 1; }
message Int32Value { int32 value = 1; }
message UInt32Value { uint32 value = 1; }
message BoolValue { bool value = 1; }
message StringValue { string value = 1; }
message BytesValue { bytes value = 1; }
message Any { string type_url = 1; bytes value = 2; }
"#;

/// The shared `TestAllTypes` body (used by all three dialects). Nested message
/// and enum definitions are declared top-level per dialect (see `nested_defs`)
/// so the compiler's type resolution can find them by simple name.
const TEST_ALL_TYPES_BODY: &str = r#"
  int32 optional_int32 = 1;
  int64 optional_int64 = 2;
  uint32 optional_uint32 = 3;
  uint64 optional_uint64 = 4;
  sint32 optional_sint32 = 5;
  sint64 optional_sint64 = 6;
  fixed32 optional_fixed32 = 7;
  fixed64 optional_fixed64 = 8;
  sfixed32 optional_sfixed32 = 9;
  sfixed64 optional_sfixed64 = 10;
  float optional_float = 11;
  double optional_double = 12;
  bool optional_bool = 13;
  string optional_string = 14;
  bytes optional_bytes = 15;

  NestedMessage optional_nested_message = 18;
  NestedEnum optional_nested_enum = 21;

  repeated int32 repeated_int32 = 31;
  repeated int64 repeated_int64 = 32;
  repeated string repeated_string = 34;
  repeated NestedMessage repeated_nested_message = 48;
  repeated NestedEnum repeated_nested_enum = 51;

  map<int32, int32> map_int32_int32 = 56;
  map<string, string> map_string_string = 58;
  map<string, NestedMessage> map_string_nested_message = 59;

  oneof oneof_field {
    uint32 oneof_uint32 = 111;
    NestedMessage oneof_nested_message = 112;
    string oneof_string = 113;
    bytes oneof_bytes = 114;
    NestedEnum oneof_enum = 115;
  }

  google.protobuf.Timestamp optional_timestamp = 80;
"#;

/// Top-level `NestedMessage` / `NestedEnum` definitions for a dialect. The
/// `optional` labels keep proto2 and editions happy; proto3 permits them too.
fn nested_defs() -> String {
    String::from(
        "message NestedMessage { optional int32 aa = 1; optional string bb = 2; }\n\
         enum NestedEnum { FOO = 0; BAR = 1; BAZ = 2; NEG = 3; }\n",
    )
}

/// Build the proto3 `TestAllTypes` source (with `optional` for explicit
/// presence coverage).
fn proto3_src() -> String {
    let mut s = String::from("syntax = \"proto3\";\npackage proto3;\nimport \"google/protobuf/wkt.proto\";\n");
    s.push_str(&nested_defs());
    s.push_str("message TestAllTypes {\n");
    s.push_str(TEST_ALL_TYPES_BODY);
    s.push_str("}\n");
    s
}

/// Build the proto2 `TestAllTypes` source (all fields `optional`).
fn proto2_src() -> String {
    let mut s = String::from("syntax = \"proto2\";\npackage proto2;\nimport \"google/protobuf/wkt.proto\";\n");
    s.push_str(&nested_defs());
    s.push_str("message TestAllTypes {\n");
    // In proto2 every scalar needs `optional`; inject it for the scalar block.
    let body = TEST_ALL_TYPES_BODY
        .replace("  int32 optional_int32 = 1;\n", "  optional int32 optional_int32 = 1;\n")
        .replace("  int64 optional_int64 = 2;\n", "  optional int64 optional_int64 = 2;\n")
        .replace("  uint32 optional_uint32 = 3;\n", "  optional uint32 optional_uint32 = 3;\n")
        .replace("  uint64 optional_uint64 = 4;\n", "  optional uint64 optional_uint64 = 4;\n")
        .replace("  sint32 optional_sint32 = 5;\n", "  optional sint32 optional_sint32 = 5;\n")
        .replace("  sint64 optional_sint64 = 6;\n", "  optional sint64 optional_sint64 = 6;\n")
        .replace("  fixed32 optional_fixed32 = 7;\n", "  optional fixed32 optional_fixed32 = 7;\n")
        .replace("  fixed64 optional_fixed64 = 8;\n", "  optional fixed64 optional_fixed64 = 8;\n")
        .replace("  sfixed32 optional_sfixed32 = 9;\n", "  optional sfixed32 optional_sfixed32 = 9;\n")
        .replace("  sfixed64 optional_sfixed64 = 10;\n", "  optional sfixed64 optional_sfixed64 = 10;\n")
        .replace("  float optional_float = 11;\n", "  optional float optional_float = 11;\n")
        .replace("  double optional_double = 12;\n", "  optional double optional_double = 12;\n")
        .replace("  bool optional_bool = 13;\n", "  optional bool optional_bool = 13;\n")
        .replace("  string optional_string = 14;\n", "  optional string optional_string = 14;\n")
        .replace("  bytes optional_bytes = 15;\n", "  optional bytes optional_bytes = 15;\n")
        .replace("  NestedMessage optional_nested_message = 18;\n", "  optional NestedMessage optional_nested_message = 18;\n")
        .replace("  NestedEnum optional_nested_enum = 21;\n", "  optional NestedEnum optional_nested_enum = 21;\n")
        .replace("  google.protobuf.Timestamp optional_timestamp = 80;\n", "  optional google.protobuf.Timestamp optional_timestamp = 80;\n");
    s.push_str(&body);
    s.push_str("}\n");
    s
}

/// Build the editions `TestAllTypes` source (`edition = "2024"`).
fn editions_src() -> String {
    let mut s = String::from("edition = \"2024\";\npackage editions;\nimport \"google/protobuf/wkt.proto\";\n");
    s.push_str(&nested_defs());
    s.push_str("message TestAllTypes {\n");
    s.push_str(TEST_ALL_TYPES_BODY);
    s.push_str("}\n");
    s
}

/// The registry of compiled schemas used by the testee and harness.
pub struct Registry {
    /// Pool containing every message and enum across all schemas.
    pub pool: DescriptorPool,
    /// Descriptor for `conformance.ConformanceRequest`.
    pub request_desc: Arc<DescriptorProto>,
    /// Descriptor for `conformance.ConformanceResponse`.
    pub response_desc: Arc<DescriptorProto>,
    /// Message descriptors indexed by fully-qualified and short name.
    messages: HashMap<String, Arc<DescriptorProto>>,
}

impl Registry {
    /// Compile all conformance/test schemas and build the registry.
    #[allow(clippy::too_many_lines)]
    pub fn build() -> Self {
        let conformance = parse_file("conformance.proto", CONFORMANCE_PROTO);
        let wkt = parse_file("google/protobuf/wkt.proto", WKT_PROTO);
        let p3 = parse_file("proto3_test.proto", &proto3_src());
        let p2 = parse_file("proto2_test.proto", &proto2_src());
        let ed = parse_file("editions_test.proto", &editions_src());

        for pr in [&conformance, &wkt, &p3, &p2, &ed] {
            if pr.diagnostics.has_errors() {
                panic!(
                    "schema '{}' parse errors: {:?}",
                    pr.file.name,
                    pr.diagnostics.iter().collect::<Vec<_>>()
                );
            }
        }

        let res = compile_set(&[
            conformance.file,
            wkt.file,
            p3.file,
            p2.file,
            ed.file,
        ]);
        assert!(
            !res.diagnostics.has_errors(),
            "schema compile errors: {:?}",
            res.diagnostics.iter().collect::<Vec<_>>()
        );

        let set: FileDescriptorSet = res.set;
        let pool = DescriptorPool::from_set(&set);

        let request_desc = pool
            .lookup_message("conformance.ConformanceRequest")
            .expect("ConformanceRequest descriptor missing");
        let response_desc = pool
            .lookup_message("conformance.ConformanceResponse")
            .expect("ConformanceResponse descriptor missing");

        let mut messages = HashMap::new();
        for f in &set.file {
            let pkg = f.package.clone().unwrap_or_default();
            for m in &f.message_type {
                index_message(m, &pkg, &mut messages);
            }
        }

        Registry {
            pool,
            request_desc,
            response_desc,
            messages,
        }
    }

    /// Look up a message descriptor by fully-qualified or short name.
    pub fn lookup(&self, name: &str) -> Option<Arc<DescriptorProto>> {
        let norm = name.trim_start_matches('.');

        // Official `conformance_test_runner` names the test messages under the
        // `protobuf_test_messages` package. Map those to our dialect names so
        // the testee can be driven directly by the reference runner instead of
        // skipping every case.
        let aliased = match norm {
            "protobuf_test_messages.proto2.TestAllTypesProto2" => "proto2.TestAllTypes",
            "protobuf_test_messages.proto3.TestAllTypesProto3" => "proto3.TestAllTypes",
            "protobuf_test_messages.editions.TestAllTypesEdition2023" => "editions.TestAllTypes",
            "protobuf_test_messages.editions.TestAllTypesEdition2024" => "editions.TestAllTypes",
            other => other,
        };

        self.messages
            .get(aliased)
            .or_else(|| {
                aliased
                    .rsplit('.')
                    .next()
                    .and_then(|short| self.messages.get(short))
            })
            .cloned()
    }
}

fn index_message(
    m: &DescriptorProto,
    prefix: &str,
    out: &mut HashMap<String, Arc<DescriptorProto>>,
) {
    let name = m.name.clone().unwrap_or_default();
    let fqn = if prefix.is_empty() {
        name.clone()
    } else {
        format!("{prefix}.{name}")
    };
    let arc = Arc::new(m.clone());
    out.insert(fqn.clone(), arc.clone());
    out.insert(name.clone(), arc);
    for n in &m.nested_type {
        index_message(n, &fqn, out);
    }
}
