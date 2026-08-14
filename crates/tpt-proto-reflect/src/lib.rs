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
    DescriptorProto, EnumDescriptorProto, FieldDescriptorProto, FieldType, FileDescriptorProto,
    FileDescriptorSet, Label,
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
    /// Extension field values keyed by field number (proto2 extensions).
    pub extensions: BTreeMap<i32, Value>,
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
///
/// It acts as both a **type registry** (messages/enums by fully-qualified
/// name) and an **extension registry** (extension fields by field number).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DescriptorPool {
    messages: std::collections::HashMap<String, Arc<DescriptorProto>>,
    enums: std::collections::HashMap<String, Arc<EnumDescriptorProto>>,
    /// Registered extension fields keyed by their field number.
    extensions: std::collections::HashMap<i32, Arc<FieldDescriptorProto>>,
    /// The syntax of the file this pool was built from (`"proto2"`,
    /// `"proto3"`, or an editions edition string).
    pub syntax: String,
}

impl DescriptorPool {
    /// Build a pool from a set of file descriptors, indexing all (nested)
    /// messages and enums by fully-qualified name, and all (nested) extensions
    /// by field number. The pool's `syntax` is taken from the first file that
    /// declares one.
    pub fn from_set(set: &FileDescriptorSet) -> DescriptorPool {
        let mut pool = DescriptorPool::default();
        for file in &set.file {
            if pool.syntax.is_empty() {
                pool.syntax = file.syntax.clone().unwrap_or_else(|| "proto3".to_string());
            }
            let pkg = file.package.as_deref().unwrap_or("");
            for m in &file.message_type {
                pool.index_message(m, pkg);
            }
            for e in &file.enum_type {
                pool.index_enum(e, pkg);
            }
            for x in &file.extension {
                pool.index_extension(x, pkg);
            }
        }
        if pool.syntax.is_empty() {
            pool.syntax = "proto3".to_string();
        }
        pool
    }

    /// Build a pool from a file descriptor, indexing all (nested) messages and
    /// enums by fully-qualified name, and all (nested) extensions by field
    /// number.
    pub fn from_file(file: &FileDescriptorProto) -> DescriptorPool {
        let mut pool = DescriptorPool::default();
        pool.syntax = file.syntax.clone().unwrap_or_else(|| "proto3".to_string());
        let pkg = file.package.as_deref().unwrap_or("");
        for m in &file.message_type {
            pool.index_message(m, pkg);
        }
        for e in &file.enum_type {
            pool.index_enum(e, pkg);
        }
        for x in &file.extension {
            pool.index_extension(x, pkg);
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
            for x in &m.extension {
                self.index_extension(x, &fqn);
            }
        }
    }

    fn index_extension(&mut self, x: &FieldDescriptorProto, prefix: &str) {
        if let Some(n) = x.number {
            self.extensions.insert(n, Arc::new(x.clone()));
        }
        let _ = prefix;
    }

    /// Look up a registered extension field by its field number.
    pub fn get_extension(&self, number: i32) -> Option<Arc<FieldDescriptorProto>> {
        self.extensions.get(&number).cloned()
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

/// Returns whether `number` falls within one of the message's extension
/// ranges (inclusive on both ends, per descriptor conventions).
fn in_extension_range(desc: &DescriptorProto, number: i32) -> bool {
    desc.extension_range
        .iter()
        .any(|r| number >= r.start && number <= r.end)
}

/// Whether a field has explicit presence semantics in the wire/dynamic model.
///
/// Messages and oneof members always have presence; proto2 `optional`/`required`
/// and proto3 explicit `optional` do; proto3 implicit scalar `optional` fields
/// do not.
fn field_has_presence(syntax: &str, f: &FieldDescriptorProto) -> bool {
    if f.label == Some(Label::Required) {
        return true;
    }
    if f.oneof_index.is_some() {
        return true;
    }
    if f.r#type == Some(FieldType::Message) || f.r#type == Some(FieldType::Group) {
        return true;
    }
    if f.proto3_optional == Some(true) {
        return true;
    }
    if syntax == "proto2" && f.label == Some(Label::Optional) {
        return true;
    }
    false
}

/// Compute the default value for a field descriptor.
fn default_value_for(pool: &DescriptorPool, f: &FieldDescriptorProto) -> Value {
    if f.label == Some(Label::Repeated) {
        let is_map = f.r#type == Some(FieldType::Message)
            && f.type_name
                .as_deref()
                .and_then(|t| pool.lookup_message(t))
                .map(|d| is_map_entry(&d))
                .unwrap_or(false);
        return if is_map { Value::Map(Vec::new()) } else { Value::List(Vec::new()) };
    }
    if let Some(FieldType::Message) | Some(FieldType::Group) = f.r#type {
        if let Some(d) = f.type_name.as_deref().and_then(|t| pool.lookup_message(t)) {
            return Value::Message(DynamicMessage::new(d, pool.clone()));
        }
    }
    match f.r#type {
        Some(FieldType::Enum) => Value::Enum(parse_enum_default(pool, f)),
        Some(t) => Value::Scalar(parse_scalar_default(t, f.default_value.as_deref())),
        None => Value::Scalar(ScalarValue::I64(0)),
    }
}

fn parse_enum_default(pool: &DescriptorPool, f: &FieldDescriptorProto) -> i32 {
    if let Some(name) = f.default_value.as_deref() {
        if let Some(e) = f.type_name.as_deref().and_then(|t| pool.lookup_enum(t)) {
            if let Some(v) = e.find_value_by_name(name) {
                return v.number.unwrap_or(0);
            }
        }
        if let Ok(n) = name.parse::<i32>() {
            return n;
        }
    }
    0
}

fn parse_scalar_default(t: FieldType, default: Option<&str>) -> ScalarValue {
    let dv = default.unwrap_or("");
    match t {
        FieldType::Int32 => ScalarValue::I64(dv.parse().unwrap_or(0)),
        FieldType::Int64 => ScalarValue::I64(dv.parse().unwrap_or(0)),
        FieldType::Sint32 => ScalarValue::I64(dv.parse().unwrap_or(0)),
        FieldType::Sint64 => ScalarValue::I64(dv.parse().unwrap_or(0)),
        FieldType::Uint32 => ScalarValue::U64(dv.parse().unwrap_or(0)),
        FieldType::Uint64 => ScalarValue::U64(dv.parse().unwrap_or(0)),
        FieldType::Fixed32 => ScalarValue::U64(dv.parse().unwrap_or(0)),
        FieldType::Fixed64 => ScalarValue::U64(dv.parse().unwrap_or(0)),
        FieldType::Sfixed32 => ScalarValue::I64(dv.parse().unwrap_or(0)),
        FieldType::Sfixed64 => ScalarValue::I64(dv.parse().unwrap_or(0)),
        FieldType::Bool => ScalarValue::Bool(dv == "true"),
        FieldType::Float => ScalarValue::F64(parse_float(dv)),
        FieldType::Double => ScalarValue::F64(parse_float(dv)),
        FieldType::String => ScalarValue::String(dv.to_string()),
        FieldType::Bytes => ScalarValue::Bytes(parse_bytes_default(dv)),
        _ => ScalarValue::I64(0),
    }
}

fn parse_float(s: &str) -> f64 {
    match s {
        "inf" | "infinity" | "+inf" | "+infinity" => f64::INFINITY,
        "-inf" | "-infinity" => f64::NEG_INFINITY,
        "nan" | "+nan" | "-nan" => f64::NAN,
        _ => s.parse().unwrap_or(0.0),
    }
}

fn parse_bytes_default(s: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            i += 1;
            match bytes[i] {
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'\\' => out.push(b'\\'),
                b'\'' => out.push(b'\''),
                b'"' => out.push(b'"'),
                b'0'..=b'7' => {
                    let mut v = 0u32;
                    let mut count = 0;
                    while count < 3 && i < bytes.len() && bytes[i].is_ascii_digit() && (bytes[i] - b'0') < 8 {
                        v = v * 8 + (bytes[i] - b'0') as u32;
                        i += 1;
                        count += 1;
                    }
                    out.push(v as u8);
                    i -= 1;
                }
                b'x' if i + 1 < bytes.len() => {
                    let mut v = 0u32;
                    let mut count = 0;
                    i += 1;
                    while count < 2 && i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                        v = v * 16 + bytes[i].to_ascii_lowercase() as char as u32 - b'0' as u32
                            + if bytes[i].is_ascii_digit() { 0 } else { 87 };
                        i += 1;
                        count += 1;
                    }
                    out.push(v as u8);
                    i -= 1;
                }
                other => out.push(other),
            }
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    out
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
            extensions: BTreeMap::new(),
            unknown: UnknownFieldSet::new(),
            pool,
        }
    }

    /// Decode a message from a reader using its descriptor.
    pub fn decode(pool: &DescriptorPool, descriptor: Arc<DescriptorProto>, reader: &mut Reader) -> Result<DynamicMessage, ReflectError> {
        let mut msg = DynamicMessage::new(descriptor.clone(), pool.clone());
        while !reader.is_empty() {
            let tag = reader.read_tag()?;
            let number = tag.field_number as i32;
            if let Some(field) = descriptor.field.iter().find(|f| f.number == Some(number)) {
                let val = decode_one(pool, field, tag.wire_type, reader)?;
                insert_field(&mut msg, field, val);
            } else if in_extension_range(&descriptor, number) {
                if let Some(ext) = pool.get_extension(number) {
                    let val = decode_one(pool, &ext, tag.wire_type, reader)?;
                    msg.extensions.insert(ext.number.unwrap_or(number), val);
                    continue;
                }
                msg.unknown.store(tag, reader)?;
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
        for (number, value) in &self.extensions {
            let field = self
                .pool
                .get_extension(*number)
                .ok_or(ReflectError::UnknownField(*number))?;
            encode_field(&self.pool, &field, value, &mut w)?;
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

    /// Set a field value by number. If the field belongs to a oneof, the other
    /// members of that oneof are cleared first (mutually exclusive).
    pub fn set_field(&mut self, number: i32, value: Value) {
        if let Some(f) = self.descriptor.find_field_by_number(number) {
            if let Some(oi) = f.oneof_index {
                let members: Vec<i32> = self
                    .descriptor
                    .field
                    .iter()
                    .filter(|g| g.oneof_index == Some(oi))
                    .filter_map(|g| g.number)
                    .collect();
                for mem in members {
                    if mem != number {
                        self.fields.remove(&mem);
                    }
                }
            }
        }
        self.fields.insert(number, value);
    }

    /// Get a field's scalar value (convenience).
    pub fn get_scalar(&self, number: i32) -> Option<&ScalarValue> {
        match self.fields.get(&number) {
            Some(Value::Scalar(s)) => Some(s),
            _ => None,
        }
    }

    /// Get an extension value by field number.
    pub fn get_extension(&self, number: i32) -> Option<&Value> {
        self.extensions.get(&number)
    }

    /// Get a mutable extension value by field number.
    pub fn get_extension_mut(&mut self, number: i32) -> Option<&mut Value> {
        self.extensions.get_mut(&number)
    }

    /// Set an extension value by field number.
    pub fn set_extension(&mut self, number: i32, value: Value) {
        self.extensions.insert(number, value);
    }

    /// Determine which field of a oneof is currently set, returning its field
    /// number, or `None` if the oneof is empty.
    pub fn which_oneof(&self, oneof_index: i32) -> Option<i32> {
        self.descriptor
            .field
            .iter()
            .find(|f| f.oneof_index == Some(oneof_index) && self.fields.contains_key(&f.number.unwrap_or(0)))
            .and_then(|f| f.number)
    }

    /// Clear a regular field (and, if it belongs to a oneof, the whole oneof).
    pub fn clear_field(&mut self, number: i32) {
        if let Some(f) = self.descriptor.find_field_by_number(number) {
            if let Some(oi) = f.oneof_index {
                let members: Vec<i32> = self
                    .descriptor
                    .field
                    .iter()
                    .filter(|g| g.oneof_index == Some(oi))
                    .filter_map(|g| g.number)
                    .collect();
                for m in members {
                    self.fields.remove(&m);
                }
                return;
            }
        }
        self.fields.remove(&number);
    }

    /// Returns `true` if the field is currently present (set), honouring
    /// presence semantics: repeated/map fields are "present" when non-empty,
    /// oneof members only when they are the active member, and other fields
    /// when they appear in the set of values.
    pub fn has_field(&self, number: i32) -> bool {
        match self.descriptor.find_field_by_number(number) {
            Some(f) if f.label == Some(Label::Repeated) => match self.fields.get(&number) {
                Some(Value::List(l)) => !l.is_empty(),
                Some(Value::Map(m)) => !m.is_empty(),
                _ => false,
            },
            Some(f) if f.oneof_index.is_some() => self.which_oneof(f.oneof_index.unwrap()).map_or(false, |active| active == number),
            _ => self.fields.contains_key(&number),
        }
    }

    /// Whether a field has explicit presence semantics (proto2 optional/
    /// required, proto3 explicit `optional`, oneof members, and message fields).
    /// Proto3 implicit scalar fields do not.
    pub fn field_has_presence(&self, number: i32) -> bool {
        match self.descriptor.find_field_by_number(number) {
            Some(f) => field_has_presence(&self.pool.syntax, f),
            None => false,
        }
    }

    /// Return the default value for a field, per its descriptor. Scalar defaults
    /// honour an explicit `default` value in the schema; enums default to `0`
    /// (or the named default); messages default to an empty message; repeated
    /// and map fields default to empty.
    pub fn default_field_value(&self, number: i32) -> Option<Value> {
        self.descriptor
            .find_field_by_number(number)
            .map(|f| default_value_for(&self.pool, f))
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
            // Propagate depth so `max_depth` is enforced across the whole
            // nesting chain (a fresh `Reader::new` would reset depth to 0).
            let mut sub_reader = reader.nested(body)?;
            let dm = DynamicMessage::decode(pool, sub, &mut sub_reader)?;
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
        (FieldType::Double, Value::Scalar(ScalarValue::F64(x))) => w.write_fixed64(x.to_bits()),
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

    fn pool_and(src: &str, msg: &str) -> (DescriptorPool, Arc<DescriptorProto>) {
        use tpt_proto_language::parse_file;
        let parsed = parse_file("t.proto", src);
        assert!(!parsed.diagnostics.has_errors(), "parse: {:?}", parsed.diagnostics.iter().collect::<Vec<_>>());
        let (fd, diags) = compile(&parsed.file);
        assert!(!diags.has_errors(), "compile: {:?}", diags.iter().collect::<Vec<_>>());
        let pool = DescriptorPool::from_file(&fd);
        let m = pool.lookup_message(msg).unwrap();
        (pool, m)
    }

    #[test]
    fn oneof_access_and_presence() {
        let (pool, m) = pool_and_person();
        let mut dm = DynamicMessage::new(m.clone(), pool.clone());
        // proto3 implicit scalar has no presence.
        assert!(!dm.field_has_presence(2));
        // oneof starts empty.
        assert_eq!(dm.which_oneof(0), None);
        assert!(!dm.has_field(5));
        dm.set_field(5, Value::Scalar(ScalarValue::String("e".into())));
        assert_eq!(dm.which_oneof(0), Some(5));
        assert!(dm.has_field(5));
        // Setting the other member clears the first.
        dm.set_field(6, Value::Scalar(ScalarValue::String("p".into())));
        assert_eq!(dm.which_oneof(0), Some(6));
        assert!(!dm.has_field(5));
        dm.clear_field(6);
        assert_eq!(dm.which_oneof(0), None);
    }

    #[test]
    fn default_value_inspection() {
        let src = r#"
syntax = "proto2";
package p2;
message Foo {
  optional int32 a = 1 [default = 42];
  required string b = 2;
  optional bool c = 3;
}
"#;
        let (pool, m) = pool_and(src, "p2.Foo");
        let dm = DynamicMessage::new(m.clone(), pool.clone());
        assert!(dm.field_has_presence(1));
        assert!(dm.field_has_presence(3));
        assert!(!dm.has_field(1));
        assert_eq!(dm.default_field_value(1), Some(Value::Scalar(ScalarValue::I64(42))));
        assert_eq!(dm.default_field_value(2), Some(Value::Scalar(ScalarValue::String(String::new()))));
        assert_eq!(dm.default_field_value(3), Some(Value::Scalar(ScalarValue::Bool(false))));
    }

    #[test]
    fn extension_roundtrip() {
        use tpt_proto_core::Reader;
        use tpt_proto_descriptor::{
            DescriptorProto, ExtensionRange, FieldDescriptorProto, FieldType, FileDescriptorProto, Label,
        };
        let mut msg = DescriptorProto::default();
        msg.name = Some("M".into());
        msg.extension_range.push(ExtensionRange { start: 100, end: 200 });

        let mut ext = FieldDescriptorProto::default();
        ext.name = Some("ext_i".into());
        ext.number = Some(100);
        ext.label = Some(Label::Optional);
        ext.r#type = Some(FieldType::Int32);
        ext.extendee = Some(".M".into());

        let mut fd = FileDescriptorProto::default();
        fd.name = Some("m.proto".into());
        fd.package = Some("p".into());
        fd.message_type.push(msg);
        fd.extension.push(ext);

        let pool = DescriptorPool::from_file(&fd);
        let m = pool.lookup_message("p.M").unwrap();
        assert!(pool.get_extension(100).is_some());

        let mut dm = DynamicMessage::new(m.clone(), pool.clone());
        dm.set_extension(100, Value::Scalar(ScalarValue::I64(7)));
        let bytes = dm.encode().unwrap();

        let mut r = Reader::new(&bytes);
        let decoded = DynamicMessage::decode(&pool, m, &mut r).unwrap();
        assert_eq!(decoded.get_extension(100), Some(&Value::Scalar(ScalarValue::I64(7))));
    }
}
