//! `tpt-proto-reflect` — descriptor-driven dynamic messages.
//!
//! [`DynamicMessage`] decodes and encodes protobuf messages purely from their
//! [`FileDescriptorProto`], without generated code. JSON, text, and tooling
//! build on top of this.

use std::collections::BTreeMap;
use std::sync::Arc;

use tpt_proto_core::{
    scalar, Reader, WireType, Writer,
};
use tpt_proto_core::UnknownFieldSet;
use tpt_proto_descriptor::{
    DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FieldType, FileDescriptorProto, Label,
};

/// A scalar value as stored in a dynamic message.
#[derive(Debug, Clone, PartialEq)]
pub enum ScalarValue {
    /// Signed 64-bit (int32/int64/sint*/sfixed*).
    I64(i64),
    /// Unsigned 64-bit (uint*/fixed*).
    U64(u64),
    /// Floating point.
    F64(f64),
    /// Boolean.
    Bool(bool),
    /// UTF-8 string.
    String(String),
    /// Arbitrary bytes.
    Bytes(Vec<u8>),
}

/// A field value in a dynamic message.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// A scalar.
    Scalar(ScalarValue),
    /// An enum value (numeric).
    Enum(i32),
    /// A nested message.
    Message(DynamicMessage),
    /// A repeated field.
    List(Vec<Value>),
    /// A map field (key, value) pairs.
    Map(Vec<(Value, Value)>),
}

/// A decoded/encoded message, driven by its descriptor.
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicMessage {
    /// The message descriptor.
    pub descriptor: Arc<DescriptorProto>,
    /// Field values keyed by field number.
    pub fields: BTreeMap<i32, Value>,
    /// Unknown fields preserved during decode.
    pub unknown: UnknownFieldSet,
    /// The descriptor pool used to resolve nested types.
    pub pool: DescriptorPool,
}

/// Errors from reflection operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReflectError {
    /// An underlying wire-format error.
    Wire(tpt_proto_core::Error),
    /// A field referenced by number was not found in the descriptor.
    UnknownField(i32),
    /// A type reference could not be resolved.
    UnresolvedType(String),
    /// A value had an unexpected variant for its field type.
    TypeMismatch(&'static str),
}

impl std::fmt::Display for ReflectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReflectError::Wire(e) => write!(f, "wire error: {e}"),
            ReflectError::UnknownField(n) => write!(f, "unknown field number {n}"),
            ReflectError::UnresolvedType(t) => write!(f, "unresolved type '{t}'"),
            ReflectError::TypeMismatch(m) => write!(f, "type mismatch: {m}"),
        }
    }
}

impl std::error::Error for ReflectError {}

impl From<tpt_proto_core::Error> for ReflectError {
    fn from(e: tpt_proto_core::Error) -> Self {
        ReflectError::Wire(e)
    }
}

/// A pool of descriptors used to resolve type references by name.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DescriptorPool {
    messages: std::collections::HashMap<String, Arc<DescriptorProto>>,
    enums: std::collections::HashMap<String, Arc<EnumDescriptorProto>>,
}

impl DescriptorPool {
    /// Build a pool from a file descriptor, indexing all (nested) messages and
    /// enums by fully-qualified name.
    pub fn from_file(file: &FileDescriptorProto) -> DescriptorPool {
        let mut pool = DescriptorPool::default();
        let pkg = file.package.as_deref().unwrap_or("");
        for m in &file.message_type {
            pool.index_message(m, pkg);
        }
        for e in &file.enum_type {
            pool.index_enum(e, pkg);
        }
        pool
    }

    fn index_message(&mut self, m: &DescriptorProto, prefix: &str) {
        let fqn = if prefix.is_empty() {
            m.name.clone().unwrap_or_default()
        } else {
            format!("{}.{}", prefix, m.name.clone().unwrap_or_default())
        };
        if m.name.is_some() {
            self.messages.insert(fqn.clone(), Arc::new(m.clone()));
            for n in &m.nested_type {
                self.index_message(n, &fqn);
            }
            for e in &m.enum_type {
                self.index_enum(e, &fqn);
            }
        }
    }

    fn index_enum(&mut self, e: &EnumDescriptorProto, prefix: &str) {
        let fqn = if prefix.is_empty() {
            e.name.clone().unwrap_or_default()
        } else {
            format!("{}.{}", prefix, e.name.clone().unwrap_or_default())
        };
        if e.name.is_some() {
            self.enums.insert(fqn, Arc::new(e.clone()));
        }
    }

    /// Look up a message descriptor by (possibly qualified) type name.
    pub fn lookup_message(&self, type_name: &str) -> Option<Arc<DescriptorProto>> {
        let norm = type_name.trim_start_matches('.');
        if let Some(m) = self.messages.get(norm) {
            return Some(m.clone());
        }
        // suffix match (e.g. "Person" within "ex.Person")
        self.messages
            .iter()
            .find(|(k, _)| k == &norm || k.ends_with(&format!(".{norm}")))
            .map(|(_, v)| v.clone())
    }

    /// Look up an enum descriptor by (possibly qualified) type name.
    pub fn lookup_enum(&self, type_name: &str) -> Option<Arc<EnumDescriptorProto>> {
        let norm = type_name.trim_start_matches('.');
        if let Some(e) = self.enums.get(norm) {
            return Some(e.clone());
        }
        self.enums
            .iter()
            .find(|(k, _)| k == &norm || k.ends_with(&format!(".{norm}")))
            .map(|(_, v)| v.clone())
    }

    /// Reverse-lookup the fully-qualified name of a message descriptor held in
    /// this pool (by `Arc` pointer identity). Used for well-known-type
    /// detection and `Any` type-URL resolution.
    pub fn full_name(&self, desc: &Arc<DescriptorProto>) -> Option<String> {
        self.messages
            .iter()
            .find(|(_, d)| Arc::ptr_eq(d, desc))
            .map(|(k, _)| k.clone())
    }

    /// Reverse-lookup the fully-qualified name of a message descriptor by value
    /// equality (used when only a borrowed descriptor is available, e.g. during
    /// JSON well-known-type detection).
    pub fn full_name_by_value(&self, desc: &DescriptorProto) -> Option<String> {
        self.messages
            .iter()
            .find(|(_, d)| ***d == *desc)
            .map(|(k, _)| k.clone())
    }

    /// Reverse-lookup the fully-qualified name of an enum descriptor held in
    /// this pool (by `Arc` pointer identity).
    pub fn enum_full_name(&self, e: &Arc<EnumDescriptorProto>) -> Option<String> {
        self.enums
            .iter()
            .find(|(_, d)| Arc::ptr_eq(d, e))
            .map(|(k, _)| k.clone())
    }
}

fn is_map_entry(desc: &DescriptorProto) -> bool {
    desc.options
        .as_deref()
        .map(|b| b.windows(2).any(|w| w == [0x38, 0x01]))
        .unwrap_or(false)
}

fn field_wire_type(t: FieldType) -> WireType {
    match t {
        FieldType::Double | FieldType::Fixed64 | FieldType::Sfixed64 => WireType::Fixed64,
        FieldType::Float | FieldType::Fixed32 | FieldType::Sfixed32 => WireType::Fixed32,
        FieldType::String | FieldType::Bytes | FieldType::Message | FieldType::Group => {
            WireType::LengthDelimited
        }
        _ => WireType::Varint,
    }
}

fn packed_allowed(t: FieldType) -> bool {
    matches!(
        t,
        FieldType::Int32
            | FieldType::Int64
            | FieldType::Uint32
            | FieldType::Uint64
            | FieldType::Sint32
            | FieldType::Sint64
            | FieldType::Bool
            | FieldType::Enum
            | FieldType::Fixed32
            | FieldType::Fixed64
            | FieldType::Sfixed32
            | FieldType::Sfixed64
            | FieldType::Float
            | FieldType::Double
    )
}

impl DynamicMessage {
    /// Create an empty message for the given descriptor and pool.
    pub fn new(descriptor: Arc<DescriptorProto>, pool: DescriptorPool) -> Self {
        DynamicMessage {
            descriptor,
            fields: BTreeMap::new(),
            unknown: UnknownFieldSet::new(),
            pool,
        }
    }

    /// Decode a message from a reader using its descriptor.
    pub fn decode(pool: &DescriptorPool, descriptor: Arc<DescriptorProto>, reader: &mut Reader) -> Result<DynamicMessage, ReflectError> {
        let mut msg = DynamicMessage::new(descriptor.clone(), pool.clone());
        while !reader.is_empty() {
            let tag = reader.read_tag()?;
            if let Some(field) = descriptor.field.iter().find(|f| f.number == Some(tag.field_number as i32)) {
                let val = decode_one(pool, field, tag.wire_type, reader)?;
                insert_field(&mut msg, field, val);
            } else {
                msg.unknown.store(tag, reader)?;
            }
        }
        Ok(msg)
    }

    /// Encode this message to bytes.
    pub fn encode(&self) -> Result<Vec<u8>, ReflectError> {
        let mut w = Writer::new();
        for (number, value) in &self.fields {
            let field = self
                .descriptor
                .field
                .iter()
                .find(|f| f.number == Some(*number))
                .ok_or(ReflectError::UnknownField(*number))?;
            encode_field(&self.pool, field, value, &mut w)?;
        }
        self.unknown.encode(&mut w);
        Ok(w.into_vec())
    }

    /// Get a field value by number.
    pub fn get_field(&self, number: i32) -> Option<&Value> {
        self.fields.get(&number)
    }

    /// Get a mutable field value by number.
    pub fn get_field_mut(&mut self, number: i32) -> Option<&mut Value> {
        self.fields.get_mut(&number)
    }

    /// Set a field value by number.
    pub fn set_field(&mut self, number: i32, value: Value) {
        self.fields.insert(number, value);
    }

    /// Get a field's scalar value (convenience).
    pub fn get_scalar(&self, number: i32) -> Option<&ScalarValue> {
        match self.fields.get(&number) {
            Some(Value::Scalar(s)) => Some(s),
            _ => None,
        }
    }
}

fn insert_field(msg: &mut DynamicMessage, field: &FieldDescriptorProto, val: Value) {
    let number = field.number.unwrap_or(0);
    let is_map = field.r#type == Some(FieldType::Message)
        && field
            .type_name
            .as_deref()
            .and_then(|t| msg.pool.lookup_message(t))
            .map(|d| is_map_entry(&d))
            .unwrap_or(false);

    if field.label == Some(Label::Repeated) {
        if is_map {
            let (k, v) = match &val {
                Value::Message(dm) => (
                    dm.fields.get(&1).cloned().unwrap_or(Value::Scalar(ScalarValue::I64(0))),
                    dm.fields.get(&2).cloned().unwrap_or(Value::Scalar(ScalarValue::I64(0))),
                ),
                _ => (
                    Value::Scalar(ScalarValue::I64(0)),
                    Value::Scalar(ScalarValue::I64(0)),
                ),
            };
            msg.fields
                .entry(number)
                .or_insert_with(|| Value::Map(Vec::new()))
                .as_map_mut()
                .push((k, v));
            return;
        }
        let entry = msg.fields.entry(number).or_insert_with(|| Value::List(Vec::new()));
        match entry {
            Value::List(l) => match val {
                Value::List(mut inner) => l.append(&mut inner),
                other => l.push(other),
            },
            _ => *entry = val,
        }
        return;
    }
    msg.fields.insert(number, val);
}

impl Value {
    fn as_map_mut(&mut self) -> &mut Vec<(Value, Value)> {
        match self {
            Value::Map(m) => m,
            _ => panic!("expected map"),
        }
    }
}

fn decode_one(
    pool: &DescriptorPool,
    field: &FieldDescriptorProto,
    wire: WireType,
    reader: &mut Reader,
) -> Result<Value, ReflectError> {
    let t = field.r#type.unwrap_or(FieldType::String);
    match t {
        FieldType::Message | FieldType::Group => {
            let body = reader.read_length_delimited()?;
            let sub = pool
                .lookup_message(field.type_name.as_deref().unwrap_or(""))
                .ok_or(ReflectError::UnresolvedType(
                    field.type_name.clone().unwrap_or_default(),
                ))?;
            let dm = DynamicMessage::decode(pool, sub, &mut Reader::new(body))?;
            Ok(Value::Message(dm))
        }
        FieldType::Enum => Ok(Value::Enum(scalar::read_int32(reader)?)),
        _ => {
            // packed?
            if wire == WireType::LengthDelimited && packed_allowed(t) && field.label == Some(Label::Repeated) {
                let body = reader.read_length_delimited()?;
                let mut sub = Reader::new(body);
                let mut out = Vec::new();
                while !sub.is_empty() {
                    out.push(decode_scalar(t, &mut sub)?);
                }
                return Ok(Value::List(out));
            }
            decode_scalar(t, reader)
        }
    }
}

fn decode_scalar(t: FieldType, reader: &mut Reader) -> Result<Value, ReflectError> {
    let v = match t {
        FieldType::Int32 => ScalarValue::I64(scalar::read_int32(reader)? as i64),
        FieldType::Int64 => ScalarValue::I64(scalar::read_int64(reader)?),
        FieldType::Uint32 => ScalarValue::U64(scalar::read_uint32(reader)? as u64),
        FieldType::Uint64 => ScalarValue::U64(scalar::read_uint64(reader)?),
        FieldType::Sint32 => ScalarValue::I64(scalar::read_sint32(reader)? as i64),
        FieldType::Sint64 => ScalarValue::I64(scalar::read_sint64(reader)?),
        FieldType::Fixed32 => ScalarValue::U64(scalar::read_fixed32(reader)? as u64),
        FieldType::Sfixed32 => ScalarValue::I64(scalar::read_sfixed32(reader)? as i64),
        FieldType::Fixed64 => ScalarValue::U64(scalar::read_fixed64(reader)?),
        FieldType::Sfixed64 => ScalarValue::I64(scalar::read_sfixed64(reader)?),
        FieldType::Float => ScalarValue::F64(scalar::read_float(reader)? as f64),
        FieldType::Double => ScalarValue::F64(scalar::read_double(reader)?),
        FieldType::Bool => ScalarValue::Bool(scalar::read_bool(reader)?),
        FieldType::String => ScalarValue::String(reader.read_string_owned()?),
        FieldType::Bytes => ScalarValue::Bytes(reader.read_length_delimited()?.to_vec()),
        FieldType::Enum => return Ok(Value::Enum(scalar::read_int32(reader)?)),
        FieldType::Message | FieldType::Group => {
            return Err(ReflectError::TypeMismatch("message handled separately"))
        }
    };
    Ok(Value::Scalar(v))
}

fn encode_field(pool: &DescriptorPool, field: &FieldDescriptorProto, value: &Value, w: &mut Writer) -> Result<(), ReflectError> {
    let number = field.number.unwrap_or(0) as u32;
    let t = field.r#type.unwrap_or(FieldType::String);
    match value {
        Value::List(items) => {
            if packed_allowed(t) {
                let mut inner = Writer::new();
                for it in items {
                    write_scalar_value(&mut inner, t, it)?;
                }
                w.write_tag(number, WireType::LengthDelimited);
                w.write_length_delimited(inner.buf());
            } else {
                for it in items {
                    w.write_tag(number, field_wire_type(t));
                    write_scalar_value(w, t, it)?;
                }
            }
        }
        Value::Map(entries) => {
            for (k, v) in entries {
                let mut entry = DynamicMessage::new(
                    pool.lookup_message(field.type_name.as_deref().unwrap_or(""))
                        .unwrap_or_else(|| Arc::new(DescriptorProto::default())),
                    pool.clone(),
                );
                entry.set_field(1, k.clone());
                entry.set_field(2, v.clone());
                let bytes = entry.encode()?;
                w.write_tag(number, WireType::LengthDelimited);
                w.write_length_delimited(&bytes);
            }
        }
        Value::Message(dm) => {
            let bytes = dm.encode()?;
            w.write_tag(number, WireType::LengthDelimited);
            w.write_length_delimited(&bytes);
        }
        Value::Enum(e) => {
            w.write_tag(number, WireType::Varint);
            w.write_varint(*e as u64);
        }
        Value::Scalar(s) => {
            w.write_tag(number, field_wire_type(t));
            write_scalar_value(w, t, &Value::Scalar(s.clone()))?;
        }
    }
    Ok(())
}

/// Write a scalar value's bytes (no tag) for the given field type.
fn write_scalar_value(w: &mut Writer, t: FieldType, value: &Value) -> Result<(), ReflectError> {
    use tpt_proto_core::{encode_zigzag32, encode_zigzag64};
    match (t, value) {
        (FieldType::Int32, Value::Scalar(ScalarValue::I64(x))) => w.write_varint(*x as u64),
        (FieldType::Int64, Value::Scalar(ScalarValue::I64(x))) => w.write_varint(*x as u64),
        (FieldType::Uint32, Value::Scalar(ScalarValue::U64(x))) => w.write_varint(*x),
        (FieldType::Uint64, Value::Scalar(ScalarValue::U64(x))) => w.write_varint(*x),
        (FieldType::Sint32, Value::Scalar(ScalarValue::I64(x))) => w.write_varint(encode_zigzag32(*x as i32) as u64),
        (FieldType::Sint64, Value::Scalar(ScalarValue::I64(x))) => w.write_varint(encode_zigzag64(*x)),
        (FieldType::Fixed32, Value::Scalar(ScalarValue::U64(x))) => w.write_fixed32(*x as u32),
        (FieldType::Sfixed32, Value::Scalar(ScalarValue::I64(x))) => w.write_fixed32(*x as u32),
        (FieldType::Fixed64, Value::Scalar(ScalarValue::U64(x))) => w.write_fixed64(*x),
        (FieldType::Sfixed64, Value::Scalar(ScalarValue::I64(x))) => w.write_fixed64(*x as u64),
        (FieldType::Float, Value::Scalar(ScalarValue::F64(x))) => w.write_fixed32((*x as f32).to_bits()),
        (FieldType::Double, Value::Scalar(ScalarValue::F64(x))) => w.write_fixed64(*x as u64),
        (FieldType::Bool, Value::Scalar(ScalarValue::Bool(b))) => w.write_varint(*b as u64),
        (FieldType::String, Value::Scalar(ScalarValue::String(s))) => w.write_length_delimited(s.as_bytes()),
        (FieldType::Bytes, Value::Scalar(ScalarValue::Bytes(b))) => w.write_length_delimited(b),
        (FieldType::Enum, Value::Enum(e)) => w.write_varint(*e as u64),
        _ => return Err(ReflectError::TypeMismatch("scalar encode")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_proto_compiler::compile;
    use tpt_proto_core::Reader;
    use tpt_proto_language::parse_file;

    const SRC: &str = r#"
syntax = "proto3";
package ex;

message Person {
  string name = 1;
  int32 id = 2;
  repeated string emails = 3;
  map<string, int32> labels = 4;
  oneof contact {
    string email = 5;
    string phone = 6;
  }
}
"#;

    fn pool_and_person() -> (DescriptorPool, Arc<DescriptorProto>) {
        let parsed = parse_file("ex.proto", SRC);
        assert!(!parsed.diagnostics.has_errors());
        let (fd, diags) = compile(&parsed.file);
        assert!(!diags.has_errors(), "diags: {:?}", diags.iter().collect::<Vec<_>>());
        let pool = DescriptorPool::from_file(&fd);
        let m = pool.lookup_message("ex.Person").unwrap();
        (pool, m)
    }

    #[test]
    fn dynamic_message_roundtrip() {
        let (pool, m) = pool_and_person();
        let mut dm = DynamicMessage::new(m.clone(), pool.clone());
        dm.set_field(1, Value::Scalar(ScalarValue::String("Alice".into())));
        dm.set_field(2, Value::Scalar(ScalarValue::I64(7)));
        dm.set_field(
            3,
            Value::List(vec![
                Value::Scalar(ScalarValue::String("a@x".into())),
                Value::Scalar(ScalarValue::String("b@y".into())),
            ]),
        );
        dm.set_field(
            4,
            Value::Map(vec![(
                Value::Scalar(ScalarValue::String("x".into())),
                Value::Scalar(ScalarValue::I64(1)),
            )]),
        );
        dm.set_field(5, Value::Scalar(ScalarValue::String("e@z".into())));

        let bytes = dm.encode().unwrap();
        let mut r = Reader::new(&bytes);
        let decoded = DynamicMessage::decode(&pool, m, &mut r).unwrap();

        assert_eq!(
            decoded.get_field(1),
            Some(&Value::Scalar(ScalarValue::String("Alice".into())))
        );
        assert_eq!(decoded.get_field(2), Some(&Value::Scalar(ScalarValue::I64(7))));
        match decoded.get_field(3) {
            Some(Value::List(items)) => assert_eq!(items.len(), 2),
            other => panic!("expected list, got {other:?}"),
        }
        match decoded.get_field(4) {
            Some(Value::Map(entries)) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].0, Value::Scalar(ScalarValue::String("x".into())));
                assert_eq!(entries[0].1, Value::Scalar(ScalarValue::I64(1)));
            }
            other => panic!("expected map, got {other:?}"),
        }
        assert_eq!(decoded.get_field(5), Some(&Value::Scalar(ScalarValue::String("e@z".into()))));
        assert!(decoded.get_field(6).is_none());
    }

    #[test]
    fn unknown_fields_preserved() {
        let (pool, m) = pool_and_person();
        let mut w = tpt_proto_core::Writer::new();
        tpt_proto_core::scalar::encode_int32(&mut w, 99, 42);
        let bytes = w.into_vec();
        let mut r = Reader::new(&bytes);
        let dm = DynamicMessage::decode(&pool, m, &mut r).unwrap();
        assert!(!dm.unknown.is_empty());
        let re = dm.encode().unwrap();
        assert_eq!(re, bytes);
    }
}
