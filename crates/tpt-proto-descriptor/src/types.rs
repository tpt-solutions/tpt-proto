//! Descriptor model for Protocol Buffers schemas.
//!
//! These types mirror `google.protobuf.*` descriptor messages and support
//! binary (de)serialization via [`tpt_proto_core`], so that a
//! `FileDescriptorSet` can round-trip through the protobuf wire format.
//!
//! Options are stored opaquely as their serialized bytes; higher layers that
//! need typed access can re-parse them.

use tpt_proto_core::{Reader, WireType, Writer};
use tpt_proto_core::Message;

/// The wire type of a field, as in `FieldDescriptorProto.Type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum FieldType {
    /// `double`
    Double = 1,
    /// `float`
    Float = 2,
    /// `int64`
    Int64 = 3,
    /// `uint64`
    Uint64 = 4,
    /// `int32`
    Int32 = 5,
    /// `fixed64`
    Fixed64 = 6,
    /// `fixed32`
    Fixed32 = 7,
    /// `bool`
    Bool = 8,
    /// `string`
    String = 9,
    /// `group` (legacy)
    Group = 10,
    /// `message`
    Message = 11,
    /// `bytes`
    Bytes = 12,
    /// `uint32`
    Uint32 = 13,
    /// `enum`
    Enum = 14,
    /// `sfixed32`
    Sfixed32 = 15,
    /// `sfixed64`
    Sfixed64 = 16,
    /// `sint32`
    Sint32 = 17,
    /// `sint64`
    Sint64 = 18,
}

impl FieldType {
    /// Parse from the integer enum value.
    pub fn from_i32(v: i32) -> Option<FieldType> {
        Some(match v {
            1 => FieldType::Double,
            2 => FieldType::Float,
            3 => FieldType::Int64,
            4 => FieldType::Uint64,
            5 => FieldType::Int32,
            6 => FieldType::Fixed64,
            7 => FieldType::Fixed32,
            8 => FieldType::Bool,
            9 => FieldType::String,
            10 => FieldType::Group,
            11 => FieldType::Message,
            12 => FieldType::Bytes,
            13 => FieldType::Uint32,
            14 => FieldType::Enum,
            15 => FieldType::Sfixed32,
            16 => FieldType::Sfixed64,
            17 => FieldType::Sint32,
            18 => FieldType::Sint64,
            _ => return None,
        })
    }

    /// The integer enum value.
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

/// The label (cardinality) of a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum Label {
    /// `optional` (or implicit in proto3).
    #[default]
    Optional = 1,
    /// `required` (proto2).
    Required = 2,
    /// `repeated`.
    Repeated = 3,
}

impl Label {
    /// Parse from the integer enum value.
    pub fn from_i32(v: i32) -> Option<Label> {
        Some(match v {
            1 => Label::Optional,
            2 => Label::Required,
            3 => Label::Repeated,
            _ => return None,
        })
    }

    /// The integer enum value.
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

/// A source-code location attached to a descriptor node.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceCodeInfo {
    /// Locations, each referencing a node via `path`.
    pub locations: Vec<Location>,
}

/// A single source location.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Location {
    /// Path of field indices identifying the referenced node.
    pub path: Vec<i32>,
    /// `[start_line, start_col, end_line?, end_col?]` (1-based, 0-based?).
    pub span: Vec<i32>,
    /// Leading comments.
    pub leading_comments: Option<String>,
    /// Trailing comments.
    pub trailing_comments: Option<String>,
    /// Detached leading comments.
    pub leading_detached_comments: Vec<String>,
}

/// A file descriptor (mirrors `FileDescriptorProto`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileDescriptorProto {
    /// File name.
    pub name: Option<String>,
    /// Package name.
    pub package: Option<String>,
    /// Imported file names.
    pub dependency: Vec<String>,
    /// Indices into `dependency` that are public.
    pub public_dependency: Vec<i32>,
    /// Indices into `dependency` that are weak.
    pub weak_dependency: Vec<i32>,
    /// Top-level messages.
    pub message_type: Vec<DescriptorProto>,
    /// Top-level enums.
    pub enum_type: Vec<EnumDescriptorProto>,
    /// Top-level services.
    pub service: Vec<ServiceDescriptorProto>,
    /// Top-level extensions.
    pub extension: Vec<FieldDescriptorProto>,
    /// Serialized `FileOptions`.
    pub options: Option<Vec<u8>>,
    /// Source code info.
    pub source_code_info: Option<SourceCodeInfo>,
    /// Syntax string (`"proto2"`, `"proto3"`, or editions edition).
    pub syntax: Option<String>,
}

/// A message descriptor (mirrors `DescriptorProto`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DescriptorProto {
    /// Message name.
    pub name: Option<String>,
    /// Regular fields.
    pub field: Vec<FieldDescriptorProto>,
    /// Nested extensions.
    pub extension: Vec<FieldDescriptorProto>,
    /// Nested messages.
    pub nested_type: Vec<DescriptorProto>,
    /// Nested enums.
    pub enum_type: Vec<EnumDescriptorProto>,
    /// Extension number ranges.
    pub extension_range: Vec<ExtensionRange>,
    /// Oneofs.
    pub oneof_decl: Vec<OneofDescriptorProto>,
    /// Serialized `MessageOptions`.
    pub options: Option<Vec<u8>>,
    /// Reserved number ranges.
    pub reserved_range: Vec<ReservedRange>,
    /// Reserved names.
    pub reserved_name: Vec<String>,
}

/// A numeric reserved range within a message/enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReservedRange {
    /// Inclusive start.
    pub start: i32,
    /// Inclusive end, or 0 for a single-element range.
    pub end: i32,
}

/// An extension number range within a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExtensionRange {
    /// Inclusive start.
    pub start: i32,
    /// Inclusive end, or 0 for a single-element range.
    pub end: i32,
}

/// A field descriptor (mirrors `FieldDescriptorProto`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FieldDescriptorProto {
    /// Field name.
    pub name: Option<String>,
    /// Field number.
    pub number: Option<i32>,
    /// Label.
    pub label: Option<Label>,
    /// Type.
    pub r#type: Option<FieldType>,
    /// Type name (for message/enum/group fields).
    pub type_name: Option<String>,
    /// Extendee (for extension fields).
    pub extendee: Option<String>,
    /// Default value (as text).
    pub default_value: Option<String>,
    /// Oneof index, if the field belongs to a oneof.
    pub oneof_index: Option<i32>,
    /// JSON name.
    pub json_name: Option<String>,
    /// Serialized `FieldOptions`.
    pub options: Option<Vec<u8>>,
    /// proto3 explicit `optional` marker.
    pub proto3_optional: Option<bool>,
}

/// An enum descriptor (mirrors `EnumDescriptorProto`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnumDescriptorProto {
    /// Enum name.
    pub name: Option<String>,
    /// Values.
    pub value: Vec<EnumValueDescriptorProto>,
    /// Serialized `EnumOptions`.
    pub options: Option<Vec<u8>>,
    /// Reserved number ranges.
    pub reserved_range: Vec<ReservedRange>,
    /// Reserved names.
    pub reserved_name: Vec<String>,
}

/// An enum value descriptor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnumValueDescriptorProto {
    /// Value name.
    pub name: Option<String>,
    /// Numeric value.
    pub number: Option<i32>,
    /// Serialized `EnumValueOptions`.
    pub options: Option<Vec<u8>>,
}

/// A oneof descriptor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OneofDescriptorProto {
    /// Oneof name.
    pub name: Option<String>,
    /// Serialized `OneofOptions`.
    pub options: Option<Vec<u8>>,
}

/// A service descriptor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ServiceDescriptorProto {
    /// Service name.
    pub name: Option<String>,
    /// Methods.
    pub method: Vec<MethodDescriptorProto>,
    /// Serialized `ServiceOptions`.
    pub options: Option<Vec<u8>>,
}

/// A method descriptor.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MethodDescriptorProto {
    /// Method name.
    pub name: Option<String>,
    /// Request type (fully-qualified, e.g. `.pkg.Msg`).
    pub input_type: Option<String>,
    /// Response type.
    pub output_type: Option<String>,
    /// Serialized `MethodOptions`.
    pub options: Option<Vec<u8>>,
    /// Client streaming.
    pub client_streaming: Option<bool>,
    /// Server streaming.
    pub server_streaming: Option<bool>,
}

/// A set of file descriptors (mirrors `FileDescriptorSet`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileDescriptorSet {
    /// The file descriptors in the set.
    pub file: Vec<FileDescriptorProto>,
}

/// Error type for descriptor binary operations.
pub type Error = tpt_proto_core::Error;

// ---------------------------------------------------------------------------
// Binary (de)serialization.
// ---------------------------------------------------------------------------

fn encode_opt_str(w: &mut Writer, field: u32, v: &Option<String>) {
    if let Some(s) = v {
        tpt_proto_core::scalar::encode_string(w, field, s);
    }
}

fn encode_repeated_str(w: &mut Writer, field: u32, items: &[String]) {
    for s in items {
        tpt_proto_core::scalar::encode_string(w, field, s);
    }
}

fn encode_repeated_i32(w: &mut Writer, field: u32, items: &[i32]) {
    for &i in items {
        tpt_proto_core::scalar::encode_int32(w, field, i);
    }
}

fn encode_opt_i32(w: &mut Writer, field: u32, v: Option<i32>) {
    if let Some(i) = v {
        tpt_proto_core::scalar::encode_int32(w, field, i);
    }
}

fn encode_opt_bool(w: &mut Writer, field: u32, v: Option<bool>) {
    if let Some(b) = v {
        tpt_proto_core::scalar::encode_bool(w, field, b);
    }
}

fn encode_opt_bytes(w: &mut Writer, field: u32, v: &Option<Vec<u8>>) {
    if let Some(b) = v {
        tpt_proto_core::scalar::encode_bytes(w, field, b);
    }
}

macro_rules! decode_repeated_str {
    ($r:expr, $out:expr) => {{
        $out.push($r.read_string_owned()?);
    }};
}

impl Message for FileDescriptorProto {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        encode_opt_str(w, 1, &self.name);
        encode_opt_str(w, 2, &self.package);
        encode_repeated_str(w, 3, &self.dependency);
        encode_repeated_i32(w, 10, &self.public_dependency);
        encode_repeated_i32(w, 11, &self.weak_dependency);
        for m in &self.message_type {
            w.write_tag(4, WireType::LengthDelimited);
            let mut inner = Writer::new();
            m.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        for e in &self.enum_type {
            w.write_tag(5, WireType::LengthDelimited);
            let mut inner = Writer::new();
            e.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        for s in &self.service {
            w.write_tag(6, WireType::LengthDelimited);
            let mut inner = Writer::new();
            s.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        for x in &self.extension {
            w.write_tag(7, WireType::LengthDelimited);
            let mut inner = Writer::new();
            x.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        encode_opt_bytes(w, 8, &self.options);
        if let Some(sci) = &self.source_code_info {
            w.write_tag(9, WireType::LengthDelimited);
            let mut inner = Writer::new();
            sci.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        encode_opt_str(w, 12, &self.syntax);
        Ok(())
    }

    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::LengthDelimited) => self.name = Some(r.read_string_owned()?),
                (2, WireType::LengthDelimited) => self.package = Some(r.read_string_owned()?),
                (3, WireType::LengthDelimited) => decode_repeated_str!(r, self.dependency),
                (10, WireType::Varint) => self.public_dependency.push(tpt_proto_core::scalar::read_int32(r)?),
                (11, WireType::Varint) => self.weak_dependency.push(tpt_proto_core::scalar::read_int32(r)?),
                (4, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = DescriptorProto::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.message_type.push(sub);
                }
                (5, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = EnumDescriptorProto::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.enum_type.push(sub);
                }
                (6, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = ServiceDescriptorProto::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.service.push(sub);
                }
                (7, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = FieldDescriptorProto::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.extension.push(sub);
                }
                (8, WireType::LengthDelimited) => self.options = Some(r.read_length_delimited()?.to_vec()),
                (9, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = SourceCodeInfo::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.source_code_info = Some(sub);
                }
                (12, WireType::LengthDelimited) => self.syntax = Some(r.read_string_owned()?),
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for SourceCodeInfo {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        for loc in &self.locations {
            w.write_tag(1, WireType::LengthDelimited);
            let mut inner = Writer::new();
            loc.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = Location::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.locations.push(sub);
                }
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for Location {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        encode_repeated_i32(w, 1, &self.path);
        encode_repeated_i32(w, 2, &self.span);
        encode_opt_str(w, 3, &self.leading_comments);
        encode_opt_str(w, 4, &self.trailing_comments);
        encode_repeated_str(w, 6, &self.leading_detached_comments);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Varint) => self.path.push(tpt_proto_core::scalar::read_int32(r)?),
                (2, WireType::Varint) => self.span.push(tpt_proto_core::scalar::read_int32(r)?),
                (3, WireType::LengthDelimited) => self.leading_comments = Some(r.read_string_owned()?),
                (4, WireType::LengthDelimited) => self.trailing_comments = Some(r.read_string_owned()?),
                (6, WireType::LengthDelimited) => decode_repeated_str!(r, self.leading_detached_comments),
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for DescriptorProto {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        encode_opt_str(w, 1, &self.name);
        for f in &self.field {
            w.write_tag(2, WireType::LengthDelimited);
            let mut inner = Writer::new();
            f.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        for x in &self.extension {
            w.write_tag(6, WireType::LengthDelimited);
            let mut inner = Writer::new();
            x.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        for n in &self.nested_type {
            w.write_tag(3, WireType::LengthDelimited);
            let mut inner = Writer::new();
            n.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        for e in &self.enum_type {
            w.write_tag(4, WireType::LengthDelimited);
            let mut inner = Writer::new();
            e.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        for er in &self.extension_range {
            w.write_tag(5, WireType::LengthDelimited);
            let mut inner = Writer::new();
            er.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        for o in &self.oneof_decl {
            w.write_tag(8, WireType::LengthDelimited);
            let mut inner = Writer::new();
            o.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        encode_opt_bytes(w, 7, &self.options);
        for rr in &self.reserved_range {
            w.write_tag(9, WireType::LengthDelimited);
            let mut inner = Writer::new();
            rr.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        encode_repeated_str(w, 10, &self.reserved_name);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::LengthDelimited) => self.name = Some(r.read_string_owned()?),
                (2, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = FieldDescriptorProto::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.field.push(sub);
                }
                (6, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = FieldDescriptorProto::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.extension.push(sub);
                }
                (3, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = DescriptorProto::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.nested_type.push(sub);
                }
                (4, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = EnumDescriptorProto::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.enum_type.push(sub);
                }
                (5, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = ExtensionRange::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.extension_range.push(sub);
                }
                (8, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = OneofDescriptorProto::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.oneof_decl.push(sub);
                }
                (7, WireType::LengthDelimited) => self.options = Some(r.read_length_delimited()?.to_vec()),
                (9, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = ReservedRange::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.reserved_range.push(sub);
                }
                (10, WireType::LengthDelimited) => decode_repeated_str!(r, self.reserved_name),
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for ReservedRange {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        encode_opt_i32(w, 1, if self.start != 0 { Some(self.start) } else { None });
        encode_opt_i32(w, 2, if self.end != 0 { Some(self.end) } else { None });
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Varint) => self.start = tpt_proto_core::scalar::read_int32(r)?,
                (2, WireType::Varint) => self.end = tpt_proto_core::scalar::read_int32(r)?,
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for ExtensionRange {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        encode_opt_i32(w, 1, if self.start != 0 { Some(self.start) } else { None });
        encode_opt_i32(w, 2, if self.end != 0 { Some(self.end) } else { None });
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::Varint) => self.start = tpt_proto_core::scalar::read_int32(r)?,
                (2, WireType::Varint) => self.end = tpt_proto_core::scalar::read_int32(r)?,
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for FieldDescriptorProto {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        encode_opt_str(w, 1, &self.name);
        encode_opt_str(w, 2, &self.extendee);
        encode_opt_i32(w, 3, self.number);
        if let Some(l) = self.label {
            tpt_proto_core::scalar::encode_int32(w, 4, l.as_i32());
        }
        if let Some(t) = self.r#type {
            tpt_proto_core::scalar::encode_int32(w, 5, t.as_i32());
        }
        encode_opt_str(w, 6, &self.type_name);
        encode_opt_str(w, 7, &self.default_value);
        encode_opt_i32(w, 9, self.oneof_index);
        encode_opt_str(w, 10, &self.json_name);
        encode_opt_bytes(w, 8, &self.options);
        encode_opt_bool(w, 17, self.proto3_optional);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::LengthDelimited) => self.name = Some(r.read_string_owned()?),
                (2, WireType::LengthDelimited) => self.extendee = Some(r.read_string_owned()?),
                (3, WireType::Varint) => self.number = Some(tpt_proto_core::scalar::read_int32(r)?),
                (4, WireType::Varint) => self.label = Label::from_i32(tpt_proto_core::scalar::read_int32(r)?),
                (5, WireType::Varint) => self.r#type = FieldType::from_i32(tpt_proto_core::scalar::read_int32(r)?),
                (6, WireType::LengthDelimited) => self.type_name = Some(r.read_string_owned()?),
                (7, WireType::LengthDelimited) => self.default_value = Some(r.read_string_owned()?),
                (8, WireType::LengthDelimited) => self.options = Some(r.read_length_delimited()?.to_vec()),
                (9, WireType::Varint) => self.oneof_index = Some(tpt_proto_core::scalar::read_int32(r)?),
                (10, WireType::LengthDelimited) => self.json_name = Some(r.read_string_owned()?),
                (17, WireType::Varint) => self.proto3_optional = Some(tpt_proto_core::scalar::read_bool(r)?),
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for EnumDescriptorProto {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        encode_opt_str(w, 1, &self.name);
        for v in &self.value {
            w.write_tag(2, WireType::LengthDelimited);
            let mut inner = Writer::new();
            v.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        encode_opt_bytes(w, 3, &self.options);
        for rr in &self.reserved_range {
            w.write_tag(4, WireType::LengthDelimited);
            let mut inner = Writer::new();
            rr.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        encode_repeated_str(w, 5, &self.reserved_name);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::LengthDelimited) => self.name = Some(r.read_string_owned()?),
                (2, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = EnumValueDescriptorProto::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.value.push(sub);
                }
                (3, WireType::LengthDelimited) => self.options = Some(r.read_length_delimited()?.to_vec()),
                (4, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = ReservedRange::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.reserved_range.push(sub);
                }
                (5, WireType::LengthDelimited) => decode_repeated_str!(r, self.reserved_name),
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for EnumValueDescriptorProto {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        encode_opt_str(w, 1, &self.name);
        encode_opt_i32(w, 2, self.number);
        encode_opt_bytes(w, 3, &self.options);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::LengthDelimited) => self.name = Some(r.read_string_owned()?),
                (2, WireType::Varint) => self.number = Some(tpt_proto_core::scalar::read_int32(r)?),
                (3, WireType::LengthDelimited) => self.options = Some(r.read_length_delimited()?.to_vec()),
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for OneofDescriptorProto {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        encode_opt_str(w, 1, &self.name);
        encode_opt_bytes(w, 2, &self.options);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::LengthDelimited) => self.name = Some(r.read_string_owned()?),
                (2, WireType::LengthDelimited) => self.options = Some(r.read_length_delimited()?.to_vec()),
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for ServiceDescriptorProto {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        encode_opt_str(w, 1, &self.name);
        for m in &self.method {
            w.write_tag(2, WireType::LengthDelimited);
            let mut inner = Writer::new();
            m.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        encode_opt_bytes(w, 3, &self.options);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::LengthDelimited) => self.name = Some(r.read_string_owned()?),
                (2, WireType::LengthDelimited) => {
                    let body = r.read_length_delimited()?;
                    let mut sub = MethodDescriptorProto::default();
                    sub.merge_from(&mut r.nested(body)?)?;
                    self.method.push(sub);
                }
                (3, WireType::LengthDelimited) => self.options = Some(r.read_length_delimited()?.to_vec()),
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for MethodDescriptorProto {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        encode_opt_str(w, 1, &self.name);
        encode_opt_str(w, 2, &self.input_type);
        encode_opt_str(w, 3, &self.output_type);
        encode_opt_bytes(w, 4, &self.options);
        encode_opt_bool(w, 5, self.client_streaming);
        encode_opt_bool(w, 6, self.server_streaming);
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            match (tag.field_number, tag.wire_type) {
                (1, WireType::LengthDelimited) => self.name = Some(r.read_string_owned()?),
                (2, WireType::LengthDelimited) => self.input_type = Some(r.read_string_owned()?),
                (3, WireType::LengthDelimited) => self.output_type = Some(r.read_string_owned()?),
                (4, WireType::LengthDelimited) => self.options = Some(r.read_length_delimited()?.to_vec()),
                (5, WireType::Varint) => self.client_streaming = Some(tpt_proto_core::scalar::read_bool(r)?),
                (6, WireType::Varint) => self.server_streaming = Some(tpt_proto_core::scalar::read_bool(r)?),
                _ => r.skip(tag.wire_type)?,
            }
        }
        Ok(())
    }
}

impl Message for FileDescriptorSet {
    fn encode(&self, w: &mut Writer) -> Result<(), Error> {
        for f in &self.file {
            w.write_tag(1, WireType::LengthDelimited);
            let mut inner = Writer::new();
            f.encode(&mut inner)?;
            w.write_length_delimited(inner.buf());
        }
        Ok(())
    }
    fn merge_from(&mut self, r: &mut Reader) -> Result<(), Error> {
        while !r.is_empty() {
            let tag = r.read_tag()?;
            if tag.field_number == 1 && tag.wire_type == WireType::LengthDelimited {
                let body = r.read_length_delimited()?;
                let mut sub = FileDescriptorProto::default();
                sub.merge_from(&mut Reader::new(body))?;
                self.file.push(sub);
            } else {
                r.skip(tag.wire_type)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FileDescriptorProto {
        FileDescriptorProto {
            name: Some("test.proto".into()),
            package: Some("pkg".into()),
            dependency: vec!["dep.proto".into()],
            syntax: Some("proto3".into()),
            message_type: vec![DescriptorProto {
                name: Some("Msg".into()),
                field: vec![FieldDescriptorProto {
                    name: Some("id".into()),
                    number: Some(1),
                    label: Some(Label::Optional),
                    r#type: Some(FieldType::Int32),
                    json_name: Some("id".into()),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            enum_type: vec![EnumDescriptorProto {
                name: Some("E".into()),
                value: vec![EnumValueDescriptorProto {
                    name: Some("A".into()),
                    number: Some(0),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn file_descriptor_roundtrip() {
        let f = sample();
        let bytes = f.encode_to_vec().unwrap();
        let decoded = FileDescriptorProto::decode(&bytes).unwrap();
        assert_eq!(f, decoded);
    }

    #[test]
    fn file_descriptor_set_roundtrip() {
        let mut set = FileDescriptorSet::default();
        set.file.push(sample());
        let bytes = set.encode_to_vec().unwrap();
        let decoded = FileDescriptorSet::decode(&bytes).unwrap();
        assert_eq!(set, decoded);
    }

    #[test]
    fn nested_message_roundtrip() {
        let f = sample();
        let bytes = f.encode_to_vec().unwrap();
        let decoded = FileDescriptorProto::decode(&bytes).unwrap();
        assert_eq!(decoded.message_type[0].name.as_deref(), Some("Msg"));
        assert_eq!(decoded.message_type[0].field[0].number, Some(1));
        assert_eq!(decoded.enum_type[0].value[0].name.as_deref(), Some("A"));
    }
}
