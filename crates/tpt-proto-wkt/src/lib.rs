//! `tpt-proto-wkt` — Protocol Buffers well-known types (§4.9, §14).
//!
//! Provides idiomatic Rust types for the `google.protobuf.*` well-known types,
//! each implementing [`tpt_proto_core::Message`] for binary wire
//! compatibility, plus a [`well_known_file_descriptor_set`] so the
//! descriptor-driven layers (reflection, JSON, text) can resolve these types
//! by name.

use std::collections::HashMap;

use tpt_proto_core::{
    scalar, Reader, WireType, Writer,
};
use tpt_proto_core::Message;

// ---------------------------------------------------------------------------
// Timestamp.
// ---------------------------------------------------------------------------

/// A `google.protobuf.Timestamp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timestamp {
    /// Seconds since the Unix epoch (1970-01-01T00:00:00Z).
    pub seconds: i64,
    /// Fractional seconds, in the range [-999,999,999, 999,999,999].
    pub nanos: i32,
}

impl Message for Timestamp {
    fn encode(&self, w: &mut Writer) -> tpt_proto_core::Result<()> {
        scalar::encode_int64(w, 1, self.seconds);
        scalar::encode_int32(w, 2, self.nanos);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Varint) => self.seconds = scalar::read_int64(r)?,
                (2, WireType::Varint) => self.nanos = scalar::read_int32(r)?,
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Duration.
// ---------------------------------------------------------------------------

/// A `google.protobuf.Duration`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Duration {
    /// Signed seconds.
    pub seconds: i64,
    /// Signed fractional seconds, in [-999,999,999, 999,999,999].
    pub nanos: i32,
}

impl Message for Duration {
    fn encode(&self, w: &mut Writer) -> tpt_proto_core::Result<()> {
        scalar::encode_int64(w, 1, self.seconds);
        scalar::encode_int32(w, 2, self.nanos);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Varint) => self.seconds = scalar::read_int64(r)?,
                (2, WireType::Varint) => self.nanos = scalar::read_int32(r)?,
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// FieldMask.
// ---------------------------------------------------------------------------

/// A `google.protobuf.FieldMask`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldMask {
    /// The set of field paths, e.g. `foo.bar`.
    pub paths: Vec<String>,
}

impl Message for FieldMask {
    fn encode(&self, w: &mut Writer) -> tpt_proto_core::Result<()> {
        for p in &self.paths {
            scalar::encode_string(w, 1, p);
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::LengthDelimited) => self.paths.push(r.read_string_owned()?),
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Wrapper types.
// ---------------------------------------------------------------------------

macro_rules! wrapper {
    ($name:ident, $inner:ty, $field:expr, $enc:ident, $dec:ident) => {
        #[doc = concat!("A `google.protobuf.", stringify!($name), "` wrapper.")]
        #[derive(Debug, Clone, Copy, PartialEq, Default)]
        pub struct $name(pub $inner);
        impl Message for $name {
            fn encode(&self, w: &mut Writer) -> tpt_proto_core::Result<()> {
                scalar::$enc(w, $field, self.0);
                Ok(())
            }
            fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
                while !r.is_empty() {
                    let tag = r.read_tag()?;
                    if tag.field_number == $field {
                        self.0 = scalar::$dec(r)?;
                    } else {
                        r.skip(tag.wire_type)?;
                    }
                }
                Ok(())
            }
        }
    };
}

wrapper!(DoubleValue, f64, 1, encode_double, read_double);
wrapper!(FloatValue, f32, 1, encode_float, read_float);
wrapper!(Int64Value, i64, 1, encode_int64, read_int64);
wrapper!(Uint64Value, u64, 1, encode_uint64, read_uint64);
wrapper!(Int32Value, i32, 1, encode_int32, read_int32);
wrapper!(Uint32Value, u32, 1, encode_uint32, read_uint32);
wrapper!(BoolValue, bool, 1, encode_bool, read_bool);

/// A `google.protobuf.StringValue` wrapper.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StringValue(pub String);
impl Message for StringValue {
    fn encode(&self, w: &mut Writer) -> tpt_proto_core::Result<()> {
        scalar::encode_string(w, 1, &self.0);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            if tag.field_number == 1 {
                self.0 = r.read_string_owned()?;
            } else {
                r.skip(tag.wire_type)?;
            }
        }
        Ok(())
    }
}

/// A `google.protobuf.BytesValue` wrapper.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BytesValue(pub Vec<u8>);
impl Message for BytesValue {
    fn encode(&self, w: &mut Writer) -> tpt_proto_core::Result<()> {
        scalar::encode_bytes(w, 1, &self.0);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            if tag.field_number == 1 {
                self.0 = r.read_length_delimited()?.to_vec();
            } else {
                r.skip(tag.wire_type)?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Empty.
// ---------------------------------------------------------------------------

/// A `google.protobuf.Empty`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Empty;
impl Message for Empty {
    fn encode(&self, _w: &mut Writer) -> tpt_proto_core::Result<()> {
        Ok(())
    }
    fn merge_from(&mut self, _r: &mut Reader) -> tpt_proto_core::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Struct / Value / ListValue.
// ---------------------------------------------------------------------------

/// A `google.protobuf.Value`.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A JSON null.
    Null,
    /// A number.
    Number(f64),
    /// A string.
    String(String),
    /// A boolean.
    Bool(bool),
    /// A structured value.
    Struct(Struct),
    /// A list value.
    List(ListValue),
}

impl Default for Value {
    fn default() -> Self {
        Value::Null
    }
}

/// A `google.protobuf.Struct`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Struct {
    /// Map of field name to value.
    pub fields: HashMap<String, Value>,
}

/// A `google.protobuf.ListValue`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ListValue {
    /// The list elements.
    pub values: Vec<Value>,
}

impl Message for Struct {
    fn encode(&self, w: &mut Writer) -> tpt_proto_core::Result<()> {
        for (k, v) in &self.fields {
            let mut entry = Writer::new();
            scalar::encode_string(&mut entry, 1, k);
            let mut val = Writer::new();
            v.encode(&mut val)?;
            scalar::encode_message(&mut entry, 2, &val.buf());
            scalar::encode_message(w, 1, &entry.buf());
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            if tag.field_number == 1 && tag.wire_type == WireType::LengthDelimited {
                let body = r.read_length_delimited()?;
                let mut sr = Reader::new(body);
                let mut key = String::new();
                let mut val = Value::Null;
                while !sr.is_empty() {
                    let t = sr.read_tag()?;
                    match (t.field_number, t.wire_type) {
                        (1, WireType::LengthDelimited) => key = sr.read_string_owned()?,
                        (2, WireType::LengthDelimited) => {
                            let vb = sr.read_length_delimited()?;
                            val = Value::decode(vb)?;
                        }
                        _ => sr.skip(t.wire_type)?,
                    }
                }
                self.fields.insert(key, val);
            } else {
                r.skip(tag.wire_type)?;
            }
        }
        Ok(())
    }
}

impl Message for Value {
    fn encode(&self, w: &mut Writer) -> tpt_proto_core::Result<()> {
        match self {
            Value::Null => scalar::encode_enum(w, 1, 0),
            Value::Number(x) => scalar::encode_double(w, 2, *x),
            Value::String(s) => scalar::encode_string(w, 3, s),
            Value::Bool(b) => scalar::encode_bool(w, 4, *b),
            Value::Struct(s) => {
                let mut inner = Writer::new();
                s.encode(&mut inner)?;
                scalar::encode_message(w, 5, &inner.buf());
            }
            Value::List(l) => {
                let mut inner = Writer::new();
                l.encode(&mut inner)?;
                scalar::encode_message(w, 6, &inner.buf());
            }
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
        *self = Value::Null;
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Varint) => {
                    let _ = scalar::read_int32(r)?;
                    *self = Value::Null;
                }
                (2, WireType::Varint) | (2, WireType::Fixed64) => *self = Value::Number(scalar::read_double(r)?),
                (3, WireType::LengthDelimited) => *self = Value::String(r.read_string_owned()?),
                (4, WireType::Varint) => *self = Value::Bool(scalar::read_bool(r)?),
                (5, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut s = Struct::default();
                    s.merge_from(&mut Reader::new(body))?;
                    *self = Value::Struct(s);
                }
                (6, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut l = ListValue::default();
                    l.merge_from(&mut Reader::new(body))?;
                    *self = Value::List(l);
                }
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for ListValue {
    fn encode(&self, w: &mut Writer) -> tpt_proto_core::Result<()> {
        for v in &self.values {
            let mut inner = Writer::new();
            v.encode(&mut inner)?;
            scalar::encode_message(w, 1, &inner.buf());
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            if tag.field_number == 1 && tag.wire_type == WireType::LengthDelimited {
                let body = r.read_length_delimited()?;
                self.values.push(Value::decode(body)?);
            } else {
                r.skip(tag.wire_type)?;
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Any.
// ---------------------------------------------------------------------------

/// A `google.protobuf.Any`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Any {
    /// The type URL, e.g. `type.googleapis.com/pkg.Msg`.
    pub type_url: String,
    /// The serialized embedded message.
    pub value: Vec<u8>,
}

impl Message for Any {
    fn encode(&self, w: &mut Writer) -> tpt_proto_core::Result<()> {
        scalar::encode_string(w, 1, &self.type_url);
        scalar::encode_bytes(w, 2, &self.value);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> tpt_proto_core::Result<()> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::LengthDelimited) => self.type_url = r.read_string_owned()?,
                (2, WireType::LengthDelimited) => self.value = r.read_length_delimited()?.to_vec(),
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Well-known descriptor set.
// ---------------------------------------------------------------------------

/// The full name prefix for well-known types.
pub const WELL_KNOWN_PACKAGE: &str = "google.protobuf";

/// Returns a [`FileDescriptorSet`] describing the well-known types, so the
/// descriptor-driven layers can resolve them by name (e.g. for JSON `Any`
/// expansion or `Struct`/`Value` handling).
pub fn well_known_file_descriptor_set() -> tpt_proto_descriptor::FileDescriptorSet {
    use tpt_proto_descriptor::{
        DescriptorProto, EnumDescriptorProto, EnumValueDescriptorProto, FieldDescriptorProto,
        FieldType, FileDescriptorProto, Label,
    };
    fn fld(name: &str, number: i32, ty: FieldType, label: Label, type_name: Option<&str>) -> FieldDescriptorProto {
        FieldDescriptorProto {
            name: Some(name.into()),
            number: Some(number),
            label: Some(label),
            r#type: Some(ty),
            type_name: type_name.map(|s| s.into()),
            json_name: Some(name.into()),
            ..Default::default()
        }
    }

    // NullValue enum (used by Value).
    let null_value = EnumDescriptorProto {
        name: Some("NullValue".into()),
        value: vec![EnumValueDescriptorProto {
            name: Some("NULL_VALUE".into()),
            number: Some(0),
            ..Default::default()
        }],
        ..Default::default()
    };

    let mut messages = Vec::new();

    // Timestamp / Duration / FieldMask / Empty.
    messages.push(DescriptorProto {
        name: Some("Timestamp".into()),
        field: vec![
            fld("seconds", 1, FieldType::Int64, Label::Optional, None),
            fld("nanos", 2, FieldType::Int32, Label::Optional, None),
        ],
        ..Default::default()
    });
    messages.push(DescriptorProto {
        name: Some("Duration".into()),
        field: vec![
            fld("seconds", 1, FieldType::Int64, Label::Optional, None),
            fld("nanos", 2, FieldType::Int32, Label::Optional, None),
        ],
        ..Default::default()
    });
    messages.push(DescriptorProto {
        name: Some("FieldMask".into()),
        field: vec![fld("paths", 1, FieldType::String, Label::Repeated, None)],
        ..Default::default()
    });
    messages.push(DescriptorProto {
        name: Some("Empty".into()),
        ..Default::default()
    });

    // Wrappers.
    for (n, t) in [
        ("DoubleValue", FieldType::Double),
        ("FloatValue", FieldType::Float),
        ("Int64Value", FieldType::Int64),
        ("Uint64Value", FieldType::Uint64),
        ("Int32Value", FieldType::Int32),
        ("Uint32Value", FieldType::Uint32),
        ("BoolValue", FieldType::Bool),
        ("StringValue", FieldType::String),
        ("BytesValue", FieldType::Bytes),
    ] {
        messages.push(DescriptorProto {
            name: Some(n.into()),
            field: vec![fld("value", 1, t, Label::Optional, None)],
            ..Default::default()
        });
    }

    // Struct / Value / ListValue.
    messages.push(DescriptorProto {
        name: Some("Struct".into()),
        field: vec![fld("fields", 1, FieldType::Message, Label::Repeated, Some(".google.protobuf.Struct.FieldsEntry"))],
        nested_type: vec![DescriptorProto {
            name: Some("FieldsEntry".into()),
            field: vec![
                fld("key", 1, FieldType::String, Label::Optional, None),
                fld("value", 2, FieldType::Message, Label::Optional, Some(".google.protobuf.Value")),
            ],
            options: Some(vec![0x38u8, 0x01u8]),
            ..Default::default()
        }],
        ..Default::default()
    });
    messages.push(DescriptorProto {
        name: Some("Value".into()),
        field: vec![
            fld("null_value", 1, FieldType::Enum, Label::Optional, Some(".google.protobuf.NullValue")),
            fld("number_value", 2, FieldType::Double, Label::Optional, None),
            fld("string_value", 3, FieldType::String, Label::Optional, None),
            fld("bool_value", 4, FieldType::Bool, Label::Optional, None),
            fld("struct_value", 5, FieldType::Message, Label::Optional, Some(".google.protobuf.Struct")),
            fld("list_value", 6, FieldType::Message, Label::Optional, Some(".google.protobuf.ListValue")),
        ],
        ..Default::default()
    });
    messages.push(DescriptorProto {
        name: Some("ListValue".into()),
        field: vec![fld("values", 1, FieldType::Message, Label::Repeated, Some(".google.protobuf.Value"))],
        ..Default::default()
    });

    // Any.
    messages.push(DescriptorProto {
        name: Some("Any".into()),
        field: vec![
            fld("type_url", 1, FieldType::String, Label::Optional, None),
            fld("value", 2, FieldType::Bytes, Label::Optional, None),
        ],
        ..Default::default()
    });

    let file = FileDescriptorProto {
        name: Some("google/protobuf/wrappers.proto".into()),
        package: Some(WELL_KNOWN_PACKAGE.into()),
        message_type: messages,
        enum_type: vec![null_value],
        syntax: Some("proto3".into()),
        ..Default::default()
    };
    tpt_proto_descriptor::FileDescriptorSet { file: vec![file] }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timestamp_roundtrip() {
        let t = Timestamp { seconds: 1_700_000_000, nanos: 123_456_789 };
        let bytes = t.encode_to_vec().unwrap();
        let back = Timestamp::decode(&bytes).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn duration_roundtrip() {
        let d = Duration { seconds: -3, nanos: 250_000_000 };
        let bytes = d.encode_to_vec().unwrap();
        let back = Duration::decode(&bytes).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn field_mask_roundtrip() {
        let m = FieldMask { paths: vec!["a.b".into(), "c".into()] };
        let bytes = m.encode_to_vec().unwrap();
        let back = FieldMask::decode(&bytes).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn wrapper_roundtrip() {
        let w = Int64Value(42);
        let bytes = w.encode_to_vec().unwrap();
        assert_eq!(Int64Value::decode(&bytes).unwrap(), w);
        let s = StringValue("hi".into());
        assert_eq!(StringValue::decode(&s.encode_to_vec().unwrap()).unwrap(), s);
    }

    #[test]
    fn struct_roundtrip() {
        let mut s = Struct::default();
        s.fields.insert("n".into(), Value::Number(1.5));
        s.fields.insert("s".into(), Value::String("x".into()));
        let bytes = s.encode_to_vec().unwrap();
        let back = Struct::decode(&bytes).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn any_roundtrip() {
        let a = Any { type_url: "type.googleapis.com/foo.Bar".into(), value: vec![1, 2, 3] };
        let bytes = a.encode_to_vec().unwrap();
        assert_eq!(Any::decode(&bytes).unwrap(), a);
    }

    #[test]
    fn descriptor_set_builds() {
        let set = well_known_file_descriptor_set();
        assert!(!set.file.is_empty());
    }

    #[test]
    fn json_struct_via_descriptors() {
        use tpt_proto_json::{message_to_json_string, json_string_to_message, JsonOptions};
        use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue as RScalar, Value};

        let set = well_known_file_descriptor_set();
        let file = &set.file[0];
        let pool = DescriptorPool::from_file(file);
        let struct_desc = pool.lookup_message("google.protobuf.Struct").unwrap();

        // Build a Struct: { "a": 1, "b": "x" }
        let mut dm = DynamicMessage::new(struct_desc.clone(), pool.clone());
        let mut vmsg = DynamicMessage::new(pool.lookup_message("google.protobuf.Value").unwrap(), pool.clone());
        vmsg.set_field(2, Value::Scalar(RScalar::F64(1.0)));
        dm.set_field(1, Value::Map(vec![(
            Value::Scalar(RScalar::String("a".into())),
            Value::Message(vmsg),
        )]));

        let opts = JsonOptions::default();
        let json = message_to_json_string(&pool, &struct_desc, &dm, &opts).unwrap();
        assert!(json == r#"{"a":1}"# || json == r#"{"a":1.0}"#);

        let back = json_string_to_message(&pool, &struct_desc, &json, &opts).unwrap();
        let bytes1 = dm.encode().unwrap();
        let bytes2 = back.encode().unwrap();
        assert_eq!(bytes1, bytes2);
    }
}
