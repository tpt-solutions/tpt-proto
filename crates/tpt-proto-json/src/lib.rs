//! `tpt-proto-json` — Protocol Buffers JSON mapping (§4.7, §12).
//!
//! Converts between descriptor-driven [`DynamicMessage`] values and
//! `serde_json::Value`, honoring the canonical protobuf JSON mapping rules and
//! the well-known-types special forms (Timestamp, Duration, FieldMask, Any,
//! Struct/Value/ListValue, wrapper types, Empty).

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::{Map, Number, Value as Json};
use base64::Engine;
use tpt_proto_descriptor::{
    DescriptorProto, FieldDescriptorProto, FieldType, Label,
};
use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue as RScalar, Value};

/// Errors raised while converting to/from JSON.
#[derive(Debug, thiserror::Error)]
pub enum JsonError {
    /// An unknown field name appeared in the JSON input.
    #[error("unknown field `{0}`")]
    UnknownField(String),
    /// A value was of the wrong JSON shape for its schema type.
    #[error("type mismatch for field `{0}`: {1}")]
    TypeMismatch(String, String),
    /// A 64-bit integer string could not be parsed.
    #[error("invalid number for field `{0}`: {1}")]
    InvalidNumber(String, String),
    /// An enum value name was not found in the schema.
    #[error("unknown enum value `{0}` for field `{1}`")]
    UnknownEnum(String, String),
    /// A well-known-type conversion failed.
    #[error("well-known type error: {0}")]
    WellKnown(String),
    /// A referenced type could not be resolved.
    #[error("unresolved type `{0}`")]
    UnresolvedType(String),
    /// Nesting depth exceeded the configured limit.
    #[error("recursion limit: {0}")]
    RecursionLimit(String),
}

/// Options controlling JSON emission and parsing.
#[derive(Debug, Clone)]
pub struct JsonOptions {
    /// Emit field names in `lowerCamelCase` (proto3 JSON default). When false,
    /// the original proto field name is used.
    pub use_lower_camel_case: bool,
    /// Always emit the original proto field names (overrides `use_lower_camel_case`).
    pub use_proto_field_names: bool,
    /// Emit enum values as integers rather than by name.
    pub enum_as_ints: bool,
    /// Emit fields even when they hold their default value (proto2 JSON behavior).
    pub always_print_primitive_fields: bool,
    /// Emit 64-bit integers as JSON strings (avoids precision loss).
    pub int64_as_string: bool,
    /// Emit bytes as base64 (true) or as RFC-4648 URL-safe base64 (false).
    pub bytes_as_base64: bool,
    /// Enforce canonical JSON: sorted object keys, no extra whitespace.
    pub canonical: bool,
    /// Ignore JSON fields not present in the schema instead of erroring.
    pub ignore_unknown_fields: bool,
    /// Maximum nesting depth accepted while converting JSON. Deeply nested JSON
    /// (including `google.protobuf.Struct`/`Value`/`ListValue` cycles) is rejected
    /// rather than recursing without bound. Defaults to 100.
    ///
    /// Note: this bounds *conversion* recursion. The initial `serde_json`
    /// token parse step is performed by `serde_json` itself and remains subject
    /// to that library's own limits; this guard applies on top during the
    /// schema-driven value conversion.
    pub max_depth: u32,
}

impl Default for JsonOptions {
    fn default() -> Self {
        JsonOptions {
            use_lower_camel_case: true,
            use_proto_field_names: false,
            enum_as_ints: false,
            always_print_primitive_fields: false,
            int64_as_string: true,
            bytes_as_base64: true,
            canonical: false,
            ignore_unknown_fields: false,
            max_depth: 100,
        }
    }
}

/// Convert a [`DynamicMessage`] into its JSON representation.
pub fn message_to_json(
    pool: &DescriptorPool,
    descriptor: &DescriptorProto,
    msg: &DynamicMessage,
    opts: &JsonOptions,
) -> Result<Json, JsonError> {
    message_to_json_impl(pool, descriptor, msg, opts, 0)
}

fn message_to_json_impl(
    pool: &DescriptorPool,
    descriptor: &DescriptorProto,
    msg: &DynamicMessage,
    opts: &JsonOptions,
    depth: u32,
) -> Result<Json, JsonError> {
    if depth > opts.max_depth {
        return Err(JsonError::RecursionLimit(format!(
            "JSON nesting exceeds max_depth {}",
            opts.max_depth
        )));
    }
    if let Some(v) = well_known_to_json(pool, descriptor, msg, opts, depth)? {
        return Ok(v);
    }
    let mut out = Map::new();
    for field in &descriptor.field {
        if field.extendee.is_some() {
            continue;
        }
        let num = field.number.unwrap_or(0);
        let name = field_json_name(field, opts);
        if is_map_field(pool, field) {
            let Some(value) = msg.get_field(num) else {
                if opts.always_print_primitive_fields {
                    out.insert(name, Json::Object(Map::new()));
                }
                continue;
            };
            let Some((kf, vf)) = map_entry_fields(pool, field) else {
                return Err(JsonError::WellKnown(format!(
                    "cannot resolve map entry for {}",
                    field.name.as_deref().unwrap_or("?")
                )));
            };
            let Value::Map(entries) = value else {
                return Err(JsonError::TypeMismatch(name, "expected map".into()));
            };
            let mut obj = Map::new();
            for (k, v) in entries {
                let key = map_key_to_json(&kf, k, opts)?;
                obj.insert(key, value_to_json(pool, &vf, v, opts, depth + 1)?);
            }
            out.insert(name, Json::Object(obj));
            continue;
        }
        let Some(value) = msg.get_field(num) else {
            if opts.always_print_primitive_fields {
                if let Some(d) = default_json_value(pool, field, opts) {
                    out.insert(name, d);
                }
            }
            continue;
        };
        out.insert(name, value_to_json(pool, field, value, opts, depth + 1)?);
    }
    Ok(Json::Object(out))
}

fn value_to_json(
    pool: &DescriptorPool,
    field: &FieldDescriptorProto,
    value: &Value,
    opts: &JsonOptions,
    depth: u32,
) -> Result<Json, JsonError> {
    let t = field.r#type.unwrap_or(FieldType::String);
    match value {
        Value::List(items) => {
            let mut arr = Vec::with_capacity(items.len());
            for it in items {
                arr.push(scalar_or_complex_to_json(pool, field, t, it, opts, depth + 1)?);
            }
            Ok(Json::Array(arr))
        }
        other => scalar_or_complex_to_json(pool, field, t, other, opts, depth + 1),
    }
}

fn scalar_or_complex_to_json(
    pool: &DescriptorPool,
    field: &FieldDescriptorProto,
    t: FieldType,
    value: &Value,
    opts: &JsonOptions,
    depth: u32,
) -> Result<Json, JsonError> {
    let fname = field.name.clone().unwrap_or_default();
    match t {
        FieldType::Message | FieldType::Group => {
            let Value::Message(dm) = value else {
                return Err(JsonError::TypeMismatch(fname, "expected message".into()));
            };
            let sub = pool
                .lookup_message(field.type_name.as_deref().unwrap_or(""))
                .ok_or_else(|| JsonError::UnresolvedType(field.type_name.clone().unwrap_or_default()))?;
            message_to_json_impl(pool, &sub, dm, opts, depth + 1)
        }
        FieldType::Enum => {
            let Value::Enum(n) = value else {
                return Err(JsonError::TypeMismatch(fname, "expected enum".into()));
            };
            enum_to_json(pool, field, *n, opts)
        }
        _ => {
            let Value::Scalar(s) = value else {
                return Err(JsonError::TypeMismatch(fname, "expected scalar".into()));
            };
            scalar_to_json(t, s, opts)
        }
    }
}

fn enum_to_json(
    pool: &DescriptorPool,
    field: &FieldDescriptorProto,
    n: i32,
    opts: &JsonOptions,
) -> Result<Json, JsonError> {
    if opts.enum_as_ints {
        return Ok(Json::Number(Number::from(n)));
    }
    let name = field.type_name.as_deref().unwrap_or("");
    if let Some(edesc) = pool.lookup_enum(name) {
        if let Some(v) = edesc.value.iter().find(|v| v.number == Some(n)) {
            return Ok(Json::String(v.name.clone().unwrap_or_else(|| n.to_string())));
        }
    }
    Ok(Json::Number(Number::from(n)))
}

fn scalar_to_json(t: FieldType, s: &RScalar, opts: &JsonOptions) -> Result<Json, JsonError> {
    use RScalar::*;
    match (t, s) {
        (FieldType::String, String(x)) => Ok(Json::String(x.clone())),
        (FieldType::String, _) => Err(JsonError::TypeMismatch("string".into(), "not a string".into())),
        (FieldType::Bytes, Bytes(b)) => Ok(Json::String(if opts.bytes_as_base64 {
            base64::engine::general_purpose::STANDARD.encode(b)
        } else {
            base64::engine::general_purpose::URL_SAFE.encode(b)
        })),
        (FieldType::Bool, Bool(b)) => Ok(Json::Bool(*b)),
        (FieldType::Double, F64(x)) | (FieldType::Float, F64(x)) => float_to_json(*x),
        (FieldType::Int32, I64(x))
        | (FieldType::Int64, I64(x))
        | (FieldType::Sint32, I64(x))
        | (FieldType::Sint64, I64(x))
        | (FieldType::Sfixed32, I64(x))
        | (FieldType::Sfixed64, I64(x)) => {
            if opts.int64_as_string {
                Ok(Json::String(x.to_string()))
            } else {
                number_from_i64(*x)
            }
        }
        (FieldType::Uint32, U64(x))
        | (FieldType::Uint64, U64(x))
        | (FieldType::Fixed32, U64(x))
        | (FieldType::Fixed64, U64(x)) => {
            if opts.int64_as_string {
                Ok(Json::String(x.to_string()))
            } else {
                number_from_u64(*x)
            }
        }
        _ => Err(JsonError::TypeMismatch("scalar".into(), "type/value mismatch".into())),
    }
}

fn number_from_i64(x: i64) -> Result<Json, JsonError> {
    Number::from_f64(x as f64)
        .map(Json::Number)
        .ok_or_else(|| JsonError::InvalidNumber("int".into(), x.to_string()))
}

fn number_from_u64(x: u64) -> Result<Json, JsonError> {
    Number::from_f64(x as f64)
        .map(Json::Number)
        .ok_or_else(|| JsonError::InvalidNumber("uint".into(), x.to_string()))
}

fn float_to_json(x: f64) -> Result<Json, JsonError> {
    if x.is_nan() {
        return Ok(Json::String("NaN".into()));
    }
    if x.is_infinite() {
        return Ok(Json::String(if x > 0.0 { "Infinity".into() } else { "-Infinity".into() }));
    }
    Number::from_f64(x)
        .map(Json::Number)
        .ok_or_else(|| JsonError::InvalidNumber("float".into(), x.to_string()))
}

fn default_json_value(
    pool: &DescriptorPool,
    field: &FieldDescriptorProto,
    opts: &JsonOptions,
) -> Option<Json> {
    let t = field.r#type.unwrap_or(FieldType::String);
    match t {
        FieldType::Message | FieldType::Group => None,
        FieldType::Enum => Some(enum_to_json(pool, field, 0, opts).ok()?),
        FieldType::String => Some(Json::String(String::new())),
        FieldType::Bytes => Some(Json::String(String::new())),
        FieldType::Bool => Some(Json::Bool(false)),
        FieldType::Double | FieldType::Float => Some(Json::Number(Number::from_f64(0.0)?)),
        _ => Some(Json::Number(Number::from(0))),
    }
}

fn map_key_to_json(kf: &FieldDescriptorProto, k: &Value, _opts: &JsonOptions) -> Result<String, JsonError> {
    let kt = kf.r#type.unwrap_or(FieldType::String);
    match (kt, k) {
        (FieldType::String, Value::Scalar(RScalar::String(s))) => Ok(s.clone()),
        (FieldType::Bool, Value::Scalar(RScalar::Bool(b))) => Ok(b.to_string()),
        (FieldType::Int32, Value::Scalar(RScalar::I64(x)))
        | (FieldType::Int64, Value::Scalar(RScalar::I64(x)))
        | (FieldType::Sint32, Value::Scalar(RScalar::I64(x)))
        | (FieldType::Sint64, Value::Scalar(RScalar::I64(x)))
        | (FieldType::Sfixed32, Value::Scalar(RScalar::I64(x)))
        | (FieldType::Sfixed64, Value::Scalar(RScalar::I64(x))) => Ok(x.to_string()),
        (FieldType::Uint32, Value::Scalar(RScalar::U64(x)))
        | (FieldType::Uint64, Value::Scalar(RScalar::U64(x)))
        | (FieldType::Fixed32, Value::Scalar(RScalar::U64(x)))
        | (FieldType::Fixed64, Value::Scalar(RScalar::U64(x))) => Ok(x.to_string()),
        _ => Err(JsonError::TypeMismatch("map key".into(), "unsupported key type".into())),
    }
}

// ---------------------------------------------------------------------------
// JSON -> DynamicMessage.
// ---------------------------------------------------------------------------

/// Parse a JSON value into a [`DynamicMessage`] of the given descriptor.
pub fn json_to_message(
    pool: &DescriptorPool,
    descriptor: &DescriptorProto,
    json: &Json,
    opts: &JsonOptions,
) -> Result<DynamicMessage, JsonError> {
    json_to_message_impl(pool, descriptor, json, opts, 0)
}

fn json_to_message_impl(
    pool: &DescriptorPool,
    descriptor: &DescriptorProto,
    json: &Json,
    opts: &JsonOptions,
    depth: u32,
) -> Result<DynamicMessage, JsonError> {
    if depth > opts.max_depth {
        return Err(JsonError::RecursionLimit(format!(
            "JSON nesting exceeds max_depth {}",
            opts.max_depth
        )));
    }
    if let Some(dm) = well_known_from_json(pool, descriptor, json, opts, depth)? {
        return Ok(dm);
    }
    let mut msg = DynamicMessage::new(Arc::new(descriptor.clone()), pool.clone());
    let Json::Object(map) = json else {
        return Err(JsonError::TypeMismatch(descriptor.name.clone().unwrap_or_default(), "expected object".into()));
    };
    let mut used = HashSet::new();
    for field in &descriptor.field {
        if field.extendee.is_some() {
            continue;
        }
        let name = field_json_name(field, opts);
        let Some(jv) = map.get(&name) else { continue };
        used.insert(name.clone());
        if jv.is_null() {
            continue; // explicit null -> default
        }
        if is_map_field(pool, field) {
            let Some((kf, vf)) = map_entry_fields(pool, field) else {
                return Err(JsonError::WellKnown("cannot resolve map entry".into()));
            };
            let Json::Object(obj) = jv else {
                return Err(JsonError::TypeMismatch(name, "expected object for map".into()));
            };
            let mut entries = Vec::with_capacity(obj.len());
            for (k, v) in obj {
                let key_val = json_to_map_key(&kf, k)?;
                let val_val = json_to_value(pool, &vf, v, opts, depth + 1)?;
                entries.push((key_val, val_val));
            }
            msg.set_field(field.number.unwrap_or(0), Value::Map(entries));
            continue;
        }
        let val = json_to_value(pool, field, jv, opts, depth + 1)?;
        msg.set_field(field.number.unwrap_or(0), val);
    }
    if !opts.ignore_unknown_fields {
        for key in map.keys() {
            if !used.contains(key) {
                return Err(JsonError::UnknownField(key.clone()));
            }
        }
    }
    Ok(msg)
}

fn json_to_value(
    pool: &DescriptorPool,
    field: &FieldDescriptorProto,
    jv: &Json,
    opts: &JsonOptions,
    depth: u32,
) -> Result<Value, JsonError> {
    let t = field.r#type.unwrap_or(FieldType::String);
    let fname = field.name.clone().unwrap_or_default();
    match t {
        FieldType::Message | FieldType::Group => {
            let sub = pool
                .lookup_message(field.type_name.as_deref().unwrap_or(""))
                .ok_or_else(|| JsonError::UnresolvedType(field.type_name.clone().unwrap_or_default()))?;
            let dm = json_to_message_impl(pool, &sub, jv, opts, depth + 1)?;
            Ok(Value::Message(dm))
        }
        FieldType::Enum => json_to_enum(pool, field, jv),
        _ => {
            if jv.is_array() {
                let arr = jv.as_array().unwrap();
                let mut out = Vec::with_capacity(arr.len());
                for it in arr {
                    out.push(Value::Scalar(json_to_scalar(t, it, &fname, opts)?));
                }
                Ok(Value::List(out))
            } else {
                Ok(Value::Scalar(json_to_scalar(t, jv, &fname, opts)?))
            }
        }
    }
}

fn json_to_enum(pool: &DescriptorPool, field: &FieldDescriptorProto, jv: &Json) -> Result<Value, JsonError> {
    let fname = field.name.clone().unwrap_or_default();
    let name = field.type_name.as_deref().unwrap_or("");
    match jv {
        Json::Number(n) => Ok(Value::Enum(n.as_i64().unwrap_or(0) as i32)),
        Json::String(s) => {
            if let Some(edesc) = pool.lookup_enum(name) {
                if let Some(v) = edesc.value.iter().find(|v| v.name.as_deref() == Some(s)) {
                    return Ok(Value::Enum(v.number.unwrap_or(0)));
                }
            }
            if let Ok(n) = s.parse::<i32>() {
                return Ok(Value::Enum(n));
            }
            Err(JsonError::UnknownEnum(s.clone(), fname))
        }
        _ => Err(JsonError::TypeMismatch(fname, "expected number or string for enum".into())),
    }
}

fn json_to_scalar(t: FieldType, jv: &Json, fname: &str, opts: &JsonOptions) -> Result<RScalar, JsonError> {
    match t {
        FieldType::String => match jv {
            Json::String(s) => Ok(RScalar::String(s.clone())),
            _ => Err(JsonError::TypeMismatch(fname.into(), "expected string".into())),
        },
        FieldType::Bytes => match jv {
            Json::String(s) => {
                let bytes = if opts.bytes_as_base64 {
                    base64::engine::general_purpose::STANDARD.decode(s).or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(s))
                } else {
                    base64::engine::general_purpose::URL_SAFE.decode(s).or_else(|_| base64::engine::general_purpose::STANDARD.decode(s))
                };
                bytes.map(RScalar::Bytes).map_err(|_| JsonError::TypeMismatch(fname.into(), "invalid base64".into()))
            }
            _ => Err(JsonError::TypeMismatch(fname.into(), "expected string for bytes".into())),
        },
        FieldType::Bool => match jv {
            Json::Bool(b) => Ok(RScalar::Bool(*b)),
            _ => Err(JsonError::TypeMismatch(fname.into(), "expected bool".into())),
        },
        FieldType::Double | FieldType::Float => match jv {
            Json::Number(n) => n.as_f64().map(RScalar::F64).ok_or_else(|| JsonError::TypeMismatch(fname.into(), "not a number".into())),
            Json::String(s) => match s.as_str() {
                "NaN" => Ok(RScalar::F64(f64::NAN)),
                "Infinity" => Ok(RScalar::F64(f64::INFINITY)),
                "-Infinity" => Ok(RScalar::F64(f64::NEG_INFINITY)),
                _ => Err(JsonError::TypeMismatch(fname.into(), "invalid float string".into())),
            },
            _ => Err(JsonError::TypeMismatch(fname.into(), "expected number".into())),
        },
        FieldType::Int32 | FieldType::Sint32 | FieldType::Sfixed32 => {
            let x = parse_i64(jv, fname)?;
            Ok(RScalar::I64(x as i32 as i64))
        }
        FieldType::Int64 | FieldType::Sint64 | FieldType::Sfixed64 => Ok(RScalar::I64(parse_i64(jv, fname)?)),
        FieldType::Uint32 | FieldType::Fixed32 => {
            let x = parse_u64(jv, fname)?;
            Ok(RScalar::U64(x as u32 as u64))
        }
        FieldType::Uint64 | FieldType::Fixed64 => Ok(RScalar::U64(parse_u64(jv, fname)?)),
        _ => Err(JsonError::TypeMismatch(fname.into(), "unsupported scalar".into())),
    }
}

fn parse_i64(jv: &Json, fname: &str) -> Result<i64, JsonError> {
    match jv {
        Json::Number(n) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).ok_or_else(|| JsonError::InvalidNumber(fname.into(), n.to_string())),
        Json::String(s) => s.parse::<i64>().map_err(|_| JsonError::InvalidNumber(fname.into(), s.clone())),
        _ => Err(JsonError::TypeMismatch(fname.into(), "expected number".into())),
    }
}

fn parse_u64(jv: &Json, fname: &str) -> Result<u64, JsonError> {
    match jv {
        Json::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f as u64)).ok_or_else(|| JsonError::InvalidNumber(fname.into(), n.to_string())),
        Json::String(s) => s.parse::<u64>().map_err(|_| JsonError::InvalidNumber(fname.into(), s.clone())),
        _ => Err(JsonError::TypeMismatch(fname.into(), "expected number".into())),
    }
}

fn json_to_map_key(kf: &FieldDescriptorProto, k: &str) -> Result<Value, JsonError> {
    let kt = kf.r#type.unwrap_or(FieldType::String);
    match kt {
        FieldType::String => Ok(Value::Scalar(RScalar::String(k.to_string()))),
        FieldType::Bool => Ok(Value::Scalar(RScalar::Bool(k.parse::<bool>().map_err(|_| JsonError::TypeMismatch("map key".into(), "not bool".into()))?))),
        FieldType::Int32 | FieldType::Int64 | FieldType::Sint32 | FieldType::Sint64 | FieldType::Sfixed32 | FieldType::Sfixed64 => {
            Ok(Value::Scalar(RScalar::I64(k.parse::<i64>().map_err(|_| JsonError::TypeMismatch("map key".into(), "not int".into()))?)))
        }
        FieldType::Uint32 | FieldType::Uint64 | FieldType::Fixed32 | FieldType::Fixed64 => {
            Ok(Value::Scalar(RScalar::U64(k.parse::<u64>().map_err(|_| JsonError::TypeMismatch("map key".into(), "not uint".into()))?)))
        }
        _ => Err(JsonError::TypeMismatch("map key".into(), "unsupported key type".into())),
    }
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

fn field_json_name(field: &FieldDescriptorProto, opts: &JsonOptions) -> String {
    if opts.use_proto_field_names {
        return field.name.clone().unwrap_or_default();
    }
    if opts.use_lower_camel_case {
        if let Some(j) = &field.json_name {
            if !j.is_empty() {
                return j.clone();
            }
        }
    }
    field.name.clone().unwrap_or_default()
}

fn is_map_field(pool: &DescriptorPool, field: &FieldDescriptorProto) -> bool {
    field.label == Some(Label::Repeated)
        && field.r#type == Some(FieldType::Message)
        && field
            .type_name
            .as_deref()
            .and_then(|t| pool.lookup_message(t))
            .map(|d| is_map_entry(&d))
            .unwrap_or(false)
}

fn is_map_entry(desc: &DescriptorProto) -> bool {
    desc.options
        .as_deref()
        .map(|b| b.windows(2).any(|w| w == [0x38, 0x01]))
        .unwrap_or(false)
}

fn map_entry_fields(
    pool: &DescriptorPool,
    field: &FieldDescriptorProto,
) -> Option<(FieldDescriptorProto, FieldDescriptorProto)> {
    let entry = pool.lookup_message(field.type_name.as_deref()?)?;
    let k = entry.field.iter().find(|f| f.number == Some(1))?.clone();
    let v = entry.field.iter().find(|f| f.number == Some(2))?.clone();
    Some((k, v))
}

mod well_known;
pub use well_known::{well_known_from_json, well_known_to_json};

/// Convenience: serialize a [`DynamicMessage`] to a JSON string.
pub fn message_to_json_string(
    pool: &DescriptorPool,
    descriptor: &DescriptorProto,
    msg: &DynamicMessage,
    opts: &JsonOptions,
) -> Result<String, JsonError> {
    let v = message_to_json(pool, descriptor, msg, opts)?;
    serde_json::to_string(&v).map_err(|e| JsonError::WellKnown(e.to_string()))
}

/// Convenience: parse a [`DynamicMessage`] from a JSON string.
pub fn json_string_to_message(
    pool: &DescriptorPool,
    descriptor: &DescriptorProto,
    json: &str,
    opts: &JsonOptions,
) -> Result<DynamicMessage, JsonError> {
    let v: Json = serde_json::from_str(json).map_err(|e| JsonError::WellKnown(e.to_string()))?;
    json_to_message(pool, descriptor, &v, opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_proto_compiler::compile;
    use tpt_proto_core::Reader;
    use tpt_proto_language::parse_file;
    use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue as RScalar, Value};

    const SRC: &str = r#"
syntax = "proto3";
package ex;

enum Color { RED = 0; GREEN = 1; BLUE = 2; }

message Sub { int32 x = 1; string y = 2; }

message Person {
  string name = 1;
  int32 id = 2;
  repeated string emails = 3;
  map<string, int32> labels = 4;
  Color favorite = 5;
  Sub sub = 6;
  oneof contact {
    string email = 7;
    string phone = 8;
  }
}
"#;

    fn pool_person() -> (DescriptorPool, Arc<DescriptorProto>) {
        let parsed = parse_file("ex.proto", SRC);
        assert!(!parsed.diagnostics.iter().any(|d| d.severity == tpt_proto_language::Severity::Error));
        let (fd, diags) = compile(&parsed.file);
        assert!(!diags.iter().any(|d| d.severity == tpt_proto_language::Severity::Error), "diags: {:?}", diags);
        let pool = DescriptorPool::from_file(&fd);
        let m = pool.lookup_message("ex.Person").unwrap();
        (pool, m)
    }

    #[test]
    fn person_to_json() {
        let (pool, m) = pool_person();
        let mut dm = DynamicMessage::new(m.clone(), pool.clone());
        dm.set_field(1, Value::Scalar(RScalar::String("Alice".into())));
        dm.set_field(2, Value::Scalar(RScalar::I64(7)));
        dm.set_field(3, Value::List(vec![
            Value::Scalar(RScalar::String("a@x".into())),
            Value::Scalar(RScalar::String("b@y".into())),
        ]));
        dm.set_field(4, Value::Map(vec![(
            Value::Scalar(RScalar::String("home".into())),
            Value::Scalar(RScalar::I64(1)),
        )]));
        dm.set_field(5, Value::Enum(2));
        let mut sub = DynamicMessage::new(pool.lookup_message("ex.Sub").unwrap(), pool.clone());
        sub.set_field(1, Value::Scalar(RScalar::I64(9)));
        sub.set_field(2, Value::Scalar(RScalar::String("hi".into())));
        dm.set_field(6, Value::Message(sub));
        dm.set_field(7, Value::Scalar(RScalar::String("e@z".into())));

        let opts = JsonOptions::default();
        let json = message_to_json(&pool, &m, &dm, &opts).unwrap();
        let obj = json.as_object().unwrap();
        assert_eq!(obj.get("name"), Some(&Json::String("Alice".into())));
        assert_eq!(obj.get("id"), Some(&Json::String("7".into())));
        assert_eq!(obj.get("favorite"), Some(&Json::String("BLUE".into())));
        let emails = obj.get("emails").unwrap().as_array().unwrap();
        assert_eq!(emails.len(), 2);
        let labels = obj.get("labels").unwrap().as_object().unwrap();
        assert_eq!(labels.get("home"), Some(&Json::String("1".into())));
        // email (oneof) set, phone absent.
        assert!(obj.contains_key("email"));
        assert!(!obj.contains_key("phone"));
    }

    #[test]
    fn json_roundtrip() {
        let (pool, m) = pool_person();
        let json = r#"{"name":"Bob","id":"42","emails":["c@d"],"labels":{"x":"3"},"favorite":"GREEN","sub":{"x":"5","y":"z"}}"#;
        let opts = JsonOptions::default();
        let dm = json_string_to_message(&pool, &m, json, &opts).unwrap();
        assert_eq!(dm.get_field(1), Some(&Value::Scalar(RScalar::String("Bob".into()))));
        assert_eq!(dm.get_field(2), Some(&Value::Scalar(RScalar::I64(42))));
        assert_eq!(dm.get_field(5), Some(&Value::Enum(1)));
        let bytes = dm.encode().unwrap();
        let mut r = Reader::new(&bytes);
        let decoded = DynamicMessage::decode(&pool, m.clone(), &mut r).unwrap();
        let json2 = message_to_json_string(&pool, &m, &decoded, &opts).unwrap();
        assert_eq!(json2, r#"{"emails":["c@d"],"favorite":"GREEN","id":"42","labels":{"x":"3"},"name":"Bob","sub":{"x":"5","y":"z"}}"#);
    }

    #[test]
    fn unknown_field_rejected() {
        let (pool, m) = pool_person();
        let opts = JsonOptions::default();
        let res = json_string_to_message(&pool, &m, r#"{"bogus":1}"#, &opts);
        assert!(res.is_err());
    }

    #[test]
    fn well_known_timestamp() {
        use tpt_proto_descriptor::{DescriptorProto, FieldDescriptorProto, FieldType, FileDescriptorProto, Label};
        // Build a minimal google.protobuf.Timestamp descriptor and register it.
        let ts = DescriptorProto {
            name: Some("Timestamp".into()),
            field: vec![
                FieldDescriptorProto {
                    name: Some("seconds".into()),
                    number: Some(1),
                    label: Some(Label::Optional),
                    r#type: Some(FieldType::Int64),
                    json_name: Some("seconds".into()),
                    ..Default::default()
                },
                FieldDescriptorProto {
                    name: Some("nanos".into()),
                    number: Some(2),
                    label: Some(Label::Optional),
                    r#type: Some(FieldType::Int32),
                    json_name: Some("nanos".into()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let fd = FileDescriptorProto {
            name: Some("google/protobuf/timestamp.proto".into()),
            package: Some("google.protobuf".into()),
            message_type: vec![ts.clone()],
            syntax: Some("proto3".into()),
            ..Default::default()
        };
        let mut set = tpt_proto_descriptor::FileDescriptorSet::default();
        set.file.push(fd);
        let pool = DescriptorPool::from_file(&set.file[0]);
        let desc = pool.lookup_message("google.protobuf.Timestamp").unwrap();

        let mut dm = DynamicMessage::new(desc.clone(), pool.clone());
        dm.set_field(1, Value::Scalar(RScalar::I64(61))); // 1970-01-01T00:01:01Z
        dm.set_field(2, Value::Scalar(RScalar::I64(210_000_000)));
        let opts = JsonOptions::default();
        let json = message_to_json_string(&pool, &desc, &dm, &opts).unwrap();
        // Timestamp renders as an RFC3339 string, not an object.
        assert!(json.starts_with("\"1970-01-01T00:01:01"));
        assert!(json.ends_with("Z\""));

        let back = json_string_to_message(&pool, &desc, &json, &opts).unwrap();
        assert_eq!(back.get_field(1), Some(&Value::Scalar(RScalar::I64(61))));
        assert_eq!(back.get_field(2), Some(&Value::Scalar(RScalar::I64(210_000_000))));
    }
}

