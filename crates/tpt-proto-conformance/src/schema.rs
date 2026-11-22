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
    bytes protobuf_payload = 1;
    string json_payload = 2;
    string jspb_payload = 7;
    string text_payload = 8;
  }
  WireFormat requested_output_format = 3;
  string message_type = 4;
  TestCategory test_category = 5;
  bool print_unknown_fields = 9;
}

message ConformanceResponse {
  oneof result {
    string parse_error = 1;
    string runtime_error = 2;
    bytes protobuf_payload = 3;
    string json_payload = 4;
    string skipped = 5;
    string serialize_error = 6;
    string text_payload = 8;
    string timeout_error = 9;
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
enum NullValue { NULL_VALUE = 0; }
message Value {
  oneof kind {
    NullValue null_value = 1;
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

/// Top-level helper definitions for the proto3 dialect, mirroring upstream
/// `test_messages_proto3.proto` (`NestedMessage`, `NestedEnum` with a negative
/// value, `AliasedEnum` with `allow_alias`, `ForeignMessage`, `ForeignEnum`).
const PROTO3_DEFS: &str = r#"
message NestedMessage { int32 a = 1; TestAllTypesProto3 corecursive = 2; }
enum NestedEnum { FOO = 0; BAR = 1; BAZ = 2; NEG = -1; }
enum AliasedEnum {
  option allow_alias = true;
  ALIAS_FOO = 0;
  ALIAS_BAR = 1;
  ALIAS_BAZ = 2;
  MOO = 2;
  moo = 2;
  bAz = 2;
}
message ForeignMessage { int32 c = 1; }
enum ForeignEnum { FOREIGN_FOO = 0; FOREIGN_BAR = 1; FOREIGN_BAZ = 2; }
"#;

/// The full proto3 `TestAllTypes` body, field-for-field with upstream
/// `test_messages_proto3.proto` (the `TestAllTypesProto3` message).
const PROTO3_TEST_ALL_TYPES_BODY: &str = r#"
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
  ForeignMessage optional_foreign_message = 19;

  NestedEnum optional_nested_enum = 21;
  ForeignEnum optional_foreign_enum = 22;
  AliasedEnum optional_aliased_enum = 23;

  string optional_string_piece = 24;
  string optional_cord = 25;

  TestAllTypesProto3 recursive_message = 27;

  repeated int32 repeated_int32 = 31;
  repeated int64 repeated_int64 = 32;
  repeated uint32 repeated_uint32 = 33;
  repeated uint64 repeated_uint64 = 34;
  repeated sint32 repeated_sint32 = 35;
  repeated sint64 repeated_sint64 = 36;
  repeated fixed32 repeated_fixed32 = 37;
  repeated fixed64 repeated_fixed64 = 38;
  repeated sfixed32 repeated_sfixed32 = 39;
  repeated sfixed64 repeated_sfixed64 = 40;
  repeated float repeated_float = 41;
  repeated double repeated_double = 42;
  repeated bool repeated_bool = 43;
  repeated string repeated_string = 44;
  repeated bytes repeated_bytes = 45;

  repeated NestedMessage repeated_nested_message = 48;
  repeated ForeignMessage repeated_foreign_message = 49;

  repeated NestedEnum repeated_nested_enum = 51;
  repeated ForeignEnum repeated_foreign_enum = 52;

  repeated string repeated_string_piece = 54;
  repeated string repeated_cord = 55;

  repeated int32 packed_int32 = 75 [packed = true];
  repeated int64 packed_int64 = 76 [packed = true];
  repeated uint32 packed_uint32 = 77 [packed = true];
  repeated uint64 packed_uint64 = 78 [packed = true];
  repeated sint32 packed_sint32 = 79 [packed = true];
  repeated sint64 packed_sint64 = 80 [packed = true];
  repeated fixed32 packed_fixed32 = 81 [packed = true];
  repeated fixed64 packed_fixed64 = 82 [packed = true];
  repeated sfixed32 packed_sfixed32 = 83 [packed = true];
  repeated sfixed64 packed_sfixed64 = 84 [packed = true];
  repeated float packed_float = 85 [packed = true];
  repeated double packed_double = 86 [packed = true];
  repeated bool packed_bool = 87 [packed = true];
  repeated NestedEnum packed_nested_enum = 88 [packed = true];

  repeated int32 unpacked_int32 = 89 [packed = false];
  repeated int64 unpacked_int64 = 90 [packed = false];
  repeated uint32 unpacked_uint32 = 91 [packed = false];
  repeated uint64 unpacked_uint64 = 92 [packed = false];
  repeated sint32 unpacked_sint32 = 93 [packed = false];
  repeated sint64 unpacked_sint64 = 94 [packed = false];
  repeated fixed32 unpacked_fixed32 = 95 [packed = false];
  repeated fixed64 unpacked_fixed64 = 96 [packed = false];
  repeated sfixed32 unpacked_sfixed32 = 97 [packed = false];
  repeated sfixed64 unpacked_sfixed64 = 98 [packed = false];
  repeated float unpacked_float = 99 [packed = false];
  repeated double unpacked_double = 100 [packed = false];
  repeated bool unpacked_bool = 101 [packed = false];
  repeated NestedEnum unpacked_nested_enum = 102 [packed = false];

  map<int32, int32> map_int32_int32 = 56;
  map<int64, int64> map_int64_int64 = 57;
  map<uint32, uint32> map_uint32_uint32 = 58;
  map<uint64, uint64> map_uint64_uint64 = 59;
  map<sint32, sint32> map_sint32_sint32 = 60;
  map<sint64, sint64> map_sint64_sint64 = 61;
  map<fixed32, fixed32> map_fixed32_fixed32 = 62;
  map<fixed64, fixed64> map_fixed64_fixed64 = 63;
  map<sfixed32, sfixed32> map_sfixed32_sfixed32 = 64;
  map<sfixed64, sfixed64> map_sfixed64_sfixed64 = 65;
  map<int32, float> map_int32_float = 66;
  map<int32, double> map_int32_double = 67;
  map<bool, bool> map_bool_bool = 68;
  map<string, string> map_string_string = 69;
  map<string, bytes> map_string_bytes = 70;
  map<string, NestedMessage> map_string_nested_message = 71;
  map<string, ForeignMessage> map_string_foreign_message = 72;
  map<string, NestedEnum> map_string_nested_enum = 73;
  map<string, ForeignEnum> map_string_foreign_enum = 74;

  oneof oneof_field {
    uint32 oneof_uint32 = 111;
    NestedMessage oneof_nested_message = 112;
    string oneof_string = 113;
    bytes oneof_bytes = 114;
    bool oneof_bool = 115;
    uint64 oneof_uint64 = 116;
    float oneof_float = 117;
    double oneof_double = 118;
    NestedEnum oneof_enum = 119;
    google.protobuf.NullValue oneof_null_value = 120;
  }

  google.protobuf.BoolValue optional_bool_wrapper = 201;
  google.protobuf.Int32Value optional_int32_wrapper = 202;
  google.protobuf.Int64Value optional_int64_wrapper = 203;
  google.protobuf.UInt32Value optional_uint32_wrapper = 204;
  google.protobuf.UInt64Value optional_uint64_wrapper = 205;
  google.protobuf.FloatValue optional_float_wrapper = 206;
  google.protobuf.DoubleValue optional_double_wrapper = 207;
  google.protobuf.StringValue optional_string_wrapper = 208;
  google.protobuf.BytesValue optional_bytes_wrapper = 209;

  repeated google.protobuf.BoolValue repeated_bool_wrapper = 211;
  repeated google.protobuf.Int32Value repeated_int32_wrapper = 212;
  repeated google.protobuf.Int64Value repeated_int64_wrapper = 213;
  repeated google.protobuf.UInt32Value repeated_uint32_wrapper = 214;
  repeated google.protobuf.UInt64Value repeated_uint64_wrapper = 215;
  repeated google.protobuf.FloatValue repeated_float_wrapper = 216;
  repeated google.protobuf.DoubleValue repeated_double_wrapper = 217;
  repeated google.protobuf.StringValue repeated_string_wrapper = 218;
  repeated google.protobuf.BytesValue repeated_bytes_wrapper = 219;

  google.protobuf.Duration optional_duration = 301;
  google.protobuf.Timestamp optional_timestamp = 302;
  google.protobuf.FieldMask optional_field_mask = 303;
  google.protobuf.Struct optional_struct = 304;
  google.protobuf.Any optional_any = 305;
  google.protobuf.Value optional_value = 306;
  google.protobuf.NullValue optional_null_value = 307;

  repeated google.protobuf.Duration repeated_duration = 311;
  repeated google.protobuf.Timestamp repeated_timestamp = 312;
  repeated google.protobuf.FieldMask repeated_fieldmask = 313;
  repeated google.protobuf.Struct repeated_struct = 324;
  repeated google.protobuf.Any repeated_any = 315;
  repeated google.protobuf.Value repeated_value = 316;
  repeated google.protobuf.ListValue repeated_list_value = 317;

  int32 fieldname1 = 401;
  int32 field_name2 = 402;
  int32 _field_name3 = 403;
  int32 field__name4_ = 404;
  int32 field0name5 = 405;
  int32 field_0_name6 = 406;
  int32 fieldName7 = 407;
  int32 FieldName8 = 408;
  int32 field_Name9 = 409;
  int32 Field_Name10 = 410;
  int32 FIELD_NAME11 = 411;
  int32 FIELD_name12 = 412;
  int32 __field_name13 = 413;
  int32 __Field_name14 = 414;
  int32 field__name15 = 415;
  int32 field__Name16 = 416;
  int32 field_name17__ = 417;
  int32 Field_name18__ = 418;
"#;

/// Build the proto3 test source (full upstream field set). The package and
/// message name match upstream so `Any` type URLs and the official runner's
/// message names resolve directly, with no alias table.
fn proto3_src() -> String {
    let mut s = String::from(
        "syntax = \"proto3\";\npackage protobuf_test_messages.proto3;\nimport \"google/protobuf/wkt.proto\";\n",
    );
    s.push_str(PROTO3_DEFS);
    s.push_str("message TestAllTypesProto3 {\n");
    s.push_str(PROTO3_TEST_ALL_TYPES_BODY);
    s.push_str("}\n");
    s
}

/// Top-level helper definitions for the proto2 dialect, mirroring upstream
/// `test_messages_proto2.proto`.
const PROTO2_DEFS: &str = r#"
message ForeignMessageProto2 { optional int32 c = 1; }
enum ForeignEnumProto2 { FOREIGN_FOO = 0; FOREIGN_BAR = 1; FOREIGN_BAZ = 2; }
message NestedMessage { optional int32 a = 1; optional TestAllTypesProto2 corecursive = 2; }
enum NestedEnum { FOO = 0; BAR = 1; BAZ = 2; NEG = -1; }
"#;

/// The full proto2 `TestAllTypesProto2` body, field-for-field with upstream
/// `test_messages_proto2.proto`, minus the message-set helpers.
const PROTO2_TEST_ALL_TYPES_BODY: &str = r#"
  optional int32 optional_int32 = 1;
  optional int64 optional_int64 = 2;
  optional uint32 optional_uint32 = 3;
  optional uint64 optional_uint64 = 4;
  optional sint32 optional_sint32 = 5;
  optional sint64 optional_sint64 = 6;
  optional fixed32 optional_fixed32 = 7;
  optional fixed64 optional_fixed64 = 8;
  optional sfixed32 optional_sfixed32 = 9;
  optional sfixed64 optional_sfixed64 = 10;
  optional float optional_float = 11;
  optional double optional_double = 12;
  optional bool optional_bool = 13;
  optional string optional_string = 14;
  optional bytes optional_bytes = 15;

  optional NestedMessage optional_nested_message = 18;
  optional ForeignMessageProto2 optional_foreign_message = 19;

  optional NestedEnum optional_nested_enum = 21;
  optional ForeignEnumProto2 optional_foreign_enum = 22;

  optional string optional_string_piece = 24;
  optional string optional_cord = 25;

  optional TestAllTypesProto2 recursive_message = 27;

  repeated int32 repeated_int32 = 31;
  repeated int64 repeated_int64 = 32;
  repeated uint32 repeated_uint32 = 33;
  repeated uint64 repeated_uint64 = 34;
  repeated sint32 repeated_sint32 = 35;
  repeated sint64 repeated_sint64 = 36;
  repeated fixed32 repeated_fixed32 = 37;
  repeated fixed64 repeated_fixed64 = 38;
  repeated sfixed32 repeated_sfixed32 = 39;
  repeated sfixed64 repeated_sfixed64 = 40;
  repeated float repeated_float = 41;
  repeated double repeated_double = 42;
  repeated bool repeated_bool = 43;
  repeated string repeated_string = 44;
  repeated bytes repeated_bytes = 45;

  repeated NestedMessage repeated_nested_message = 48;
  repeated ForeignMessageProto2 repeated_foreign_message = 49;

  repeated NestedEnum repeated_nested_enum = 51;
  repeated ForeignEnumProto2 repeated_foreign_enum = 52;

  repeated string repeated_string_piece = 54;
  repeated string repeated_cord = 55;

  repeated int32 packed_int32 = 75 [packed = true];
  repeated int64 packed_int64 = 76 [packed = true];
  repeated uint32 packed_uint32 = 77 [packed = true];
  repeated uint64 packed_uint64 = 78 [packed = true];
  repeated sint32 packed_sint32 = 79 [packed = true];
  repeated sint64 packed_sint64 = 80 [packed = true];
  repeated fixed32 packed_fixed32 = 81 [packed = true];
  repeated fixed64 packed_fixed64 = 82 [packed = true];
  repeated sfixed32 packed_sfixed32 = 83 [packed = true];
  repeated sfixed64 packed_sfixed64 = 84 [packed = true];
  repeated float packed_float = 85 [packed = true];
  repeated double packed_double = 86 [packed = true];
  repeated bool packed_bool = 87 [packed = true];
  repeated NestedEnum packed_nested_enum = 88 [packed = true];

  repeated int32 unpacked_int32 = 89 [packed = false];
  repeated int64 unpacked_int64 = 90 [packed = false];
  repeated uint32 unpacked_uint32 = 91 [packed = false];
  repeated uint64 unpacked_uint64 = 92 [packed = false];
  repeated sint32 unpacked_sint32 = 93 [packed = false];
  repeated sint64 unpacked_sint64 = 94 [packed = false];
  repeated fixed32 unpacked_fixed32 = 95 [packed = false];
  repeated fixed64 unpacked_fixed64 = 96 [packed = false];
  repeated sfixed32 unpacked_sfixed32 = 97 [packed = false];
  repeated sfixed64 unpacked_sfixed64 = 98 [packed = false];
  repeated float unpacked_float = 99 [packed = false];
  repeated double unpacked_double = 100 [packed = false];
  repeated bool unpacked_bool = 101 [packed = false];
  repeated NestedEnum unpacked_nested_enum = 102 [packed = false];

  map<int32, int32> map_int32_int32 = 56;
  map<int64, int64> map_int64_int64 = 57;
  map<uint32, uint32> map_uint32_uint32 = 58;
  map<uint64, uint64> map_uint64_uint64 = 59;
  map<sint32, sint32> map_sint32_sint32 = 60;
  map<sint64, sint64> map_sint64_sint64 = 61;
  map<fixed32, fixed32> map_fixed32_fixed32 = 62;
  map<fixed64, fixed64> map_fixed64_fixed64 = 63;
  map<sfixed32, sfixed32> map_sfixed32_sfixed32 = 64;
  map<sfixed64, sfixed64> map_sfixed64_sfixed64 = 65;
  map<int32, bool> map_int32_bool = 104;
  map<int32, float> map_int32_float = 66;
  map<int32, double> map_int32_double = 67;
  map<int32, NestedMessage> map_int32_nested_message = 103;
  map<bool, bool> map_bool_bool = 68;
  map<string, string> map_string_string = 69;
  map<string, bytes> map_string_bytes = 70;
  map<string, NestedMessage> map_string_nested_message = 71;
  map<string, ForeignMessageProto2> map_string_foreign_message = 72;
  map<string, NestedEnum> map_string_nested_enum = 73;
  map<string, ForeignEnumProto2> map_string_foreign_enum = 74;

  oneof oneof_field {
    uint32 oneof_uint32 = 111;
    NestedMessage oneof_nested_message = 112;
    string oneof_string = 113;
    bytes oneof_bytes = 114;
    bool oneof_bool = 115;
    uint64 oneof_uint64 = 116;
    float oneof_float = 117;
    double oneof_double = 118;
    NestedEnum oneof_enum = 119;
  }

  extensions 120 to 200;

  optional group Data = 201 {
    optional int32 group_int32 = 202;
    optional uint32 group_uint32 = 203;
  }

  optional group MultiWordGroupField = 204 {
    optional int32 group_int32 = 205;
    optional uint32 group_uint32 = 206;
  }

  optional int32 default_int32 = 241 [default = -123456789];
  optional int64 default_int64 = 242 [default = -9123456789123456789];
  optional uint32 default_uint32 = 243 [default = 2123456789];
  // default_uint64 (244) and default_fixed64 (248) are omitted: the lexer
  // cannot yet represent integer literals above i64::MAX.
  optional sint32 default_sint32 = 245 [default = -123456789];
  optional sint64 default_sint64 = 246 [default = -9123456789123456789];
  optional fixed32 default_fixed32 = 247 [default = 2123456789];
  optional sfixed32 default_sfixed32 = 249 [default = -123456789];
  optional sfixed64 default_sfixed64 = 250 [default = -9123456789123456789];
  optional float default_float = 251 [default = 9e9];
  optional double default_double = 252 [default = 7e22];
  optional bool default_bool = 253 [default = true];
  optional string default_string = 254 [default = "Rosebud"];
  optional bytes default_bytes = 255 [default = "joshua"];

  optional int32 fieldname1 = 401;
  optional int32 field_name2 = 402;
  optional int32 _field_name3 = 403;
  optional int32 field__name4_ = 404;
  optional int32 field0name5 = 405;
  optional int32 field_0_name6 = 406;
  optional int32 fieldName7 = 407;
  optional int32 FieldName8 = 408;
  optional int32 field_Name9 = 409;
  optional int32 Field_Name10 = 410;
  optional int32 FIELD_NAME11 = 411;
  optional int32 FIELD_name12 = 412;
  optional int32 __field_name13 = 413;
  optional int32 __Field_name14 = 414;
  optional int32 field__name15 = 415;
  optional int32 field__Name16 = 416;
  optional int32 field_name17__ = 417;
  optional int32 Field_name18__ = 418;

  reserved 1000 to 9999;
"#;

/// Extensions of `TestAllTypesProto2` (upstream `extend` blocks).
const PROTO2_EXTENSIONS: &str = r#"
extend TestAllTypesProto2 {
  optional int32 extension_int32 = 120;
}

extend TestAllTypesProto2 {
  optional group GroupField = 121 {
    optional int32 group_int32 = 122;
    optional uint32 group_uint32 = 123;
  }
}
"#;

/// Build the proto2 test source (full upstream field set). The package and
/// message name match upstream, as for proto3.
fn proto2_src() -> String {
    let mut s = String::from(
        "syntax = \"proto2\";\npackage protobuf_test_messages.proto2;\nimport \"google/protobuf/wkt.proto\";\n",
    );
    s.push_str(PROTO2_DEFS);
    s.push_str("message TestAllTypesProto2 {\n");
    s.push_str(PROTO2_TEST_ALL_TYPES_BODY);
    s.push_str("}\n");
    s.push_str(PROTO2_EXTENSIONS);
    s
}

/// Build the editions `TestAllTypes` source (`edition = "2024"`).
fn editions_src() -> String {
    let mut s = String::from(
        "edition = \"2024\";\npackage editions;\nimport \"google/protobuf/wkt.proto\";\n",
    );
    s.push_str(&nested_defs());
    s.push_str("message TestAllTypes {\n");
    s.push_str(TEST_ALL_TYPES_BODY);
    s.push_str("}\n");
    s
}

/// The registry of compiled schemas used by the testee and harness.
pub struct Registry {
    /// Pool for the conformance and well-known-type messages.
    pub pool: DescriptorPool,
    /// Pool for the proto2 test messages (explicit presence).
    pub proto2_pool: DescriptorPool,
    /// Pool for the proto3 test messages (implicit presence for scalars).
    pub proto3_pool: DescriptorPool,
    /// Pool for the editions test messages.
    pub editions_pool: DescriptorPool,
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
        // A pool holds one syntax (proto2, proto3 or editions), which decides
        // field presence, so each dialect gets its own pool. The dialect's own
        // file comes first so the pool takes its syntax from that file.
        let p3_src = proto3_src();
        let p2_src = proto2_src();
        let ed_src = editions_src();
        let conformance = ("conformance.proto", CONFORMANCE_PROTO);
        let wkt = ("google/protobuf/wkt.proto", WKT_PROTO);

        let base_set = compile_sources(&[conformance, wkt]);
        let p3_set = compile_sources(&[("proto3_test.proto", p3_src.as_str()), conformance, wkt]);
        let p2_set = compile_sources(&[("proto2_test.proto", p2_src.as_str()), conformance, wkt]);
        let ed_set = compile_sources(&[("editions_test.proto", ed_src.as_str()), conformance, wkt]);

        let pool = DescriptorPool::from_set(&base_set);
        let proto3_pool = DescriptorPool::from_set(&p3_set);
        let proto2_pool = DescriptorPool::from_set(&p2_set);
        let editions_pool = DescriptorPool::from_set(&ed_set);

        let request_desc = pool
            .lookup_message("conformance.ConformanceRequest")
            .expect("ConformanceRequest descriptor missing");
        let response_desc = pool
            .lookup_message("conformance.ConformanceResponse")
            .expect("ConformanceResponse descriptor missing");

        let mut messages = HashMap::new();
        for set in [&base_set, &p3_set, &p2_set, &ed_set] {
            for f in &set.file {
                let pkg = f.package.clone().unwrap_or_default();
                for m in &f.message_type {
                    index_message(m, &pkg, &mut messages);
                }
            }
        }

        Registry {
            pool,
            proto2_pool,
            proto3_pool,
            editions_pool,
            request_desc,
            response_desc,
            messages,
        }
    }

    /// The pool for a test message type: each dialect resolves its own fields
    /// and presence rules. Other names (conformance messages) use the base pool.
    pub fn pool_for(&self, message_type: &str) -> &DescriptorPool {
        let norm = message_type.trim_start_matches('.');
        if norm.contains("proto2") {
            &self.proto2_pool
        } else if norm.contains("editions") {
            &self.editions_pool
        } else if norm.contains("proto3") {
            &self.proto3_pool
        } else {
            &self.pool
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

/// Parse and compile the named sources (in order) into one descriptor set.
fn compile_sources(sources: &[(&str, &str)]) -> FileDescriptorSet {
    let files: Vec<_> = sources
        .iter()
        .map(|(name, src)| {
            let parsed = parse_file(name, src);
            assert!(
                !parsed.diagnostics.has_errors(),
                "schema '{name}' parse errors: {:?}",
                parsed.diagnostics.iter().collect::<Vec<_>>()
            );
            parsed.file
        })
        .collect();
    let res = compile_set(&files);
    assert!(
        !res.diagnostics.has_errors(),
        "schema compile errors: {:?}",
        res.diagnostics.iter().collect::<Vec<_>>()
    );
    res.set
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
