//! Well-known-type (WKT) JSON forms (§4.7, §12, §14).
//!
//! Implements the protobuf JSON special encodings for the standard
//! `google.protobuf.*` well-known types: Timestamp, Duration, FieldMask,
//! Any, Struct, Value, ListValue, the wrapper types, and Empty.

use std::sync::Arc;

use base64::Engine;
use serde_json::{Map, Number};
use time::format_description::well_known::Rfc3339;
use time::{OffsetDateTime, UtcOffset};
use tpt_proto_core::Reader;
use tpt_proto_descriptor::{DescriptorProto, FieldType};
use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue as RScalar, Value};

use super::{Json, JsonError, JsonOptions};

fn wkt_name(pool: &DescriptorPool, desc: &DescriptorProto) -> Option<String> {
    let fqn = pool.full_name_by_value(desc)?;
    let trimmed = fqn.trim_start_matches('.');
    trimmed
        .strip_prefix("google.protobuf.")
        .map(|s| s.to_string())
}

/// Returns `Some(json)` if `descriptor` is a well-known type, or `None` to let
/// the normal message path handle it. Errors surface as `Err`.
pub fn well_known_to_json(
    pool: &DescriptorPool,
    descriptor: &DescriptorProto,
    msg: &DynamicMessage,
    opts: &JsonOptions,
    depth: u32,
) -> Result<Option<Json>, JsonError> {
    let Some(name) = wkt_name(pool, descriptor) else {
        return Ok(None);
    };
    let json = match name.as_str() {
        "Timestamp" => timestamp_to_json(msg)?,
        "Duration" => duration_to_json(msg)?,
        "FieldMask" => field_mask_to_json(msg)?,
        "Struct" => struct_to_json(pool, msg, opts, depth)?,
        "Value" => value_to_json(pool, msg, opts, depth)?,
        "ListValue" => list_value_to_json(pool, msg, opts, depth)?,
        "Any" => any_to_json(pool, msg, opts, depth)?,
        "Empty" => Json::Object(Map::new()),
        _wrapper if is_wrapper(&name) => wrapper_to_json(descriptor, msg, opts)?,
        _ => return Ok(None),
    };
    Ok(Some(json))
}

/// Like [`well_known_to_json`] but for parsing. Returns `Some(message)` if the
/// descriptor is a WKT handled here.
pub fn well_known_from_json(
    pool: &DescriptorPool,
    descriptor: &DescriptorProto,
    json: &Json,
    opts: &JsonOptions,
    depth: u32,
) -> Result<Option<DynamicMessage>, JsonError> {
    let Some(name) = wkt_name(pool, descriptor) else {
        return Ok(None);
    };
    let msg = match name.as_str() {
        "Timestamp" => timestamp_from_json(descriptor, json)?,
        "Duration" => duration_from_json(descriptor, json)?,
        "FieldMask" => field_mask_from_json(descriptor, json)?,
        "Struct" => struct_from_json(pool, descriptor, json, opts, depth)?,
        "Value" => value_from_json(pool, descriptor, json, opts, depth)?,
        "ListValue" => list_value_from_json(pool, descriptor, json, opts, depth)?,
        "Any" => any_from_json(pool, descriptor, json, opts, depth)?,
        "Empty" => DynamicMessage::new(Arc::new(descriptor.clone()), pool.clone()),
        _wrapper if is_wrapper(&name) => wrapper_from_json(descriptor, json)?,
        _ => return Ok(None),
    };
    Ok(Some(msg))
}

fn is_wrapper(name: &str) -> bool {
    matches!(
        name,
        "DoubleValue"
            | "FloatValue"
            | "Int64Value"
            | "UInt64Value"
            | "Int32Value"
            | "UInt32Value"
            | "BoolValue"
            | "StringValue"
            | "BytesValue"
    )
}

// ---------------------------------------------------------------------------
// Field access helpers.
// ---------------------------------------------------------------------------

fn get_i64(msg: &DynamicMessage, n: i32) -> Option<i64> {
    match msg.get_field(n)? {
        Value::Scalar(RScalar::I64(x)) => Some(*x),
        Value::Scalar(RScalar::U64(x)) => Some(*x as i64),
        _ => None,
    }
}

#[allow(dead_code)]
fn get_u64(msg: &DynamicMessage, n: i32) -> Option<u64> {
    match msg.get_field(n)? {
        Value::Scalar(RScalar::U64(x)) => Some(*x),
        Value::Scalar(RScalar::I64(x)) => Some(*x as u64),
        _ => None,
    }
}

fn get_str(msg: &DynamicMessage, n: i32) -> Option<String> {
    match msg.get_field(n)? {
        Value::Scalar(RScalar::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn get_bool(msg: &DynamicMessage, n: i32) -> Option<bool> {
    match msg.get_field(n)? {
        Value::Scalar(RScalar::Bool(b)) => Some(*b),
        _ => None,
    }
}

fn get_msg(msg: &DynamicMessage, n: i32) -> Option<&DynamicMessage> {
    match msg.get_field(n)? {
        Value::Message(m) => Some(m),
        _ => None,
    }
}

fn field_type(desc: &DescriptorProto, n: i32) -> Option<FieldType> {
    desc.field
        .iter()
        .find(|f| f.number == Some(n))
        .and_then(|f| f.r#type)
}

// ---------------------------------------------------------------------------
// Timestamp.
// ---------------------------------------------------------------------------

/// Smallest and largest Timestamp seconds: 0001-01-01T00:00:00Z to
/// 9999-12-31T23:59:59Z.
const TIMESTAMP_MIN_SECS: i64 = -62_135_596_800;
const TIMESTAMP_MAX_SECS: i64 = 253_402_300_799;
/// Largest magnitude of a Duration's seconds (10000 years).
const DURATION_MAX_SECS: i64 = 315_576_000_000;

/// The fractional-second part as the spec writes it: nothing, or 3, 6 or 9
/// digits, whichever is the shortest exact form (for example `.010`).
fn fraction_digits(nanos: u32) -> String {
    if nanos == 0 {
        return String::new();
    }
    let full = format!("{nanos:09}");
    let len = if nanos % 1_000_000 == 0 {
        3
    } else if nanos % 1_000 == 0 {
        6
    } else {
        9
    };
    format!(".{}", &full[..len])
}

fn timestamp_to_json(msg: &DynamicMessage) -> Result<Json, JsonError> {
    let secs = get_i64(msg, 1).unwrap_or(0);
    let nanos = get_i64(msg, 2).unwrap_or(0);
    if !(TIMESTAMP_MIN_SECS..=TIMESTAMP_MAX_SECS).contains(&secs)
        || !(0..1_000_000_000).contains(&nanos)
    {
        return Err(JsonError::WellKnown("Timestamp out of range".into()));
    }
    let dt = OffsetDateTime::from_unix_timestamp(secs)
        .map_err(|_| JsonError::WellKnown("invalid timestamp".into()))?;
    let base = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}",
        dt.year(),
        u8::from(dt.month()),
        dt.day(),
        dt.hour(),
        dt.minute(),
        dt.second()
    );
    Ok(Json::String(format!(
        "{base}{}Z",
        fraction_digits(nanos as u32)
    )))
}

fn timestamp_from_json(desc: &DescriptorProto, json: &Json) -> Result<DynamicMessage, JsonError> {
    let s = json
        .as_str()
        .ok_or_else(|| JsonError::TypeMismatch("Timestamp".into(), "expected string".into()))?;
    // The spec requires an uppercase 'T' separator and 'Z' designator.
    if s.len() < 20 || s.as_bytes()[10] != b'T' || s.chars().any(|c| c == 't' || c == 'z') {
        return Err(JsonError::WellKnown("invalid RFC3339 timestamp".into()));
    }
    let utc = OffsetDateTime::parse(s, &Rfc3339)
        .map_err(|e| JsonError::WellKnown(format!("invalid RFC3339: {e}")))?
        .to_offset(UtcOffset::UTC);
    let secs = utc.unix_timestamp();
    if !(TIMESTAMP_MIN_SECS..=TIMESTAMP_MAX_SECS).contains(&secs) {
        return Err(JsonError::WellKnown("Timestamp out of range".into()));
    }
    let nanos = utc.nanosecond() as i64;
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), DescriptorPool::default());
    msg.set_field(1, Value::Scalar(RScalar::I64(secs)));
    msg.set_field(2, Value::Scalar(RScalar::I64(nanos)));
    Ok(msg)
}

// ---------------------------------------------------------------------------
// Duration.
// ---------------------------------------------------------------------------

fn duration_to_json(msg: &DynamicMessage) -> Result<Json, JsonError> {
    let secs = get_i64(msg, 1).unwrap_or(0);
    let nanos = get_i64(msg, 2).unwrap_or(0);
    let in_range = secs.abs() <= DURATION_MAX_SECS;
    let same_sign = secs == 0 || nanos == 0 || (secs < 0) == (nanos < 0);
    if !in_range || !same_sign || nanos.abs() >= 1_000_000_000 {
        return Err(JsonError::WellKnown("Duration out of range".into()));
    }
    let sign = if secs < 0 || nanos < 0 { "-" } else { "" };
    let frac = fraction_digits(nanos.unsigned_abs() as u32);
    Ok(Json::String(format!(
        "{sign}{}{frac}s",
        secs.unsigned_abs()
    )))
}

fn duration_from_json(desc: &DescriptorProto, json: &Json) -> Result<DynamicMessage, JsonError> {
    let s = json
        .as_str()
        .ok_or_else(|| JsonError::TypeMismatch("Duration".into(), "expected string".into()))?;
    let trimmed = s
        .strip_suffix('s')
        .ok_or_else(|| JsonError::WellKnown("duration missing 's'".into()))?;
    let (sign, body) = if let Some(b) = trimmed.strip_prefix('-') {
        (-1i64, b)
    } else {
        (1i64, trimmed)
    };
    let (secs_str, nanos_str) = match body.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (body, None),
    };
    let secs: i64 = secs_str
        .parse()
        .map_err(|_| JsonError::WellKnown("invalid duration seconds".into()))?;
    let nanos: i64 = match nanos_str {
        Some(b) => {
            // At most nine fractional digits, all of them decimal.
            if b.is_empty() || b.len() > 9 || !b.bytes().all(|c| c.is_ascii_digit()) {
                return Err(JsonError::WellKnown("invalid duration nanos".into()));
            }
            format!("{b:0<9}")
                .parse()
                .map_err(|_| JsonError::WellKnown("invalid duration nanos".into()))?
        }
        None => 0,
    };
    let secs = sign * secs;
    let nanos = sign * nanos;
    if secs.abs() > DURATION_MAX_SECS {
        return Err(JsonError::WellKnown("Duration out of range".into()));
    }
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), DescriptorPool::default());
    msg.set_field(1, Value::Scalar(RScalar::I64(secs)));
    msg.set_field(2, Value::Scalar(RScalar::I64(nanos)));
    Ok(msg)
}

// ---------------------------------------------------------------------------
// FieldMask.
// ---------------------------------------------------------------------------

fn field_mask_to_json(msg: &DynamicMessage) -> Result<Json, JsonError> {
    let mut paths = Vec::new();
    if let Some(Value::List(items)) = msg.get_field(1) {
        for it in items {
            if let Value::Scalar(RScalar::String(s)) = it {
                paths.push(s.clone());
            }
        }
    }
    // Proto paths are snake_case; the JSON form is lowerCamelCase.
    let json_paths = paths
        .iter()
        .map(|p| field_mask_path_to_json(p))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json::String(json_paths.join(",")))
}

/// Convert one snake_case FieldMask path to lowerCamelCase. Paths that cannot
/// round-trip (uppercase letters, or an underscore not followed by a lowercase
/// letter) are an error.
fn field_mask_path_to_json(path: &str) -> Result<String, JsonError> {
    let mut out = String::with_capacity(path.len());
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c.is_ascii_uppercase() {
            return Err(JsonError::WellKnown(
                "FieldMask path has an uppercase letter".into(),
            ));
        }
        if c == '_' {
            match chars.next() {
                Some(n) if n.is_ascii_lowercase() => out.push(n.to_ascii_uppercase()),
                _ => {
                    return Err(JsonError::WellKnown(
                        "FieldMask path has an invalid underscore".into(),
                    ))
                }
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

/// Convert a lowerCamelCase FieldMask path from JSON to its snake_case form.
fn camel_to_snake(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 4);
    for c in path.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn field_mask_from_json(desc: &DescriptorProto, json: &Json) -> Result<DynamicMessage, JsonError> {
    let s = json
        .as_str()
        .ok_or_else(|| JsonError::TypeMismatch("FieldMask".into(), "expected string".into()))?;
    let items = if s.is_empty() {
        Vec::new()
    } else {
        // A JSON path may not contain underscores.
        s.split(',')
            .map(|p| {
                if p.contains('_') {
                    return Err(JsonError::WellKnown(
                        "FieldMask path contains an underscore".into(),
                    ));
                }
                Ok(Value::Scalar(RScalar::String(camel_to_snake(p))))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), DescriptorPool::default());
    msg.set_field(1, Value::List(items));
    Ok(msg)
}

// ---------------------------------------------------------------------------
// Wrapper types.
// ---------------------------------------------------------------------------

fn wrapper_to_json(
    desc: &DescriptorProto,
    msg: &DynamicMessage,
    opts: &JsonOptions,
) -> Result<Json, JsonError> {
    let t = field_type(desc, 1).unwrap_or(FieldType::String);
    let Some(v) = msg.get_field(1) else {
        return Ok(Json::Null);
    };
    match v {
        // Same scalar rules as a plain field (64-bit integers become strings).
        Value::Scalar(s) => super::scalar_to_json(t, s, opts),
        _ => Err(JsonError::TypeMismatch(
            "wrapper".into(),
            "expected scalar".into(),
        )),
    }
}

fn wrapper_from_json(desc: &DescriptorProto, json: &Json) -> Result<DynamicMessage, JsonError> {
    let t = field_type(desc, 1).unwrap_or(FieldType::String);
    // Same scalar rules as a plain field: range checks, quoted 64-bit values.
    let s = super::json_to_scalar(t, json, "value")?;
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), DescriptorPool::default());
    msg.set_field(1, Value::Scalar(s));
    Ok(msg)
}

// ---------------------------------------------------------------------------
// Struct / Value / ListValue.
// ---------------------------------------------------------------------------

fn struct_to_json(
    pool: &DescriptorPool,
    msg: &DynamicMessage,
    opts: &JsonOptions,
    depth: u32,
) -> Result<Json, JsonError> {
    let mut out = Map::new();
    if let Some(Value::Map(entries)) = msg.get_field(1) {
        let value_desc = pool.lookup_message(".google.protobuf.Value");
        for (k, v) in entries {
            let key = match k {
                Value::Scalar(RScalar::String(s)) => s.clone(),
                _ => continue,
            };
            let json_val = if let (Some(d), Value::Message(m)) = (&value_desc, v) {
                super::message_to_json_impl(pool, d, m, opts, depth + 1)?
            } else {
                Json::Null
            };
            out.insert(key, json_val);
        }
    }
    Ok(Json::Object(out))
}

fn struct_from_json(
    pool: &DescriptorPool,
    desc: &DescriptorProto,
    json: &Json,
    opts: &JsonOptions,
    depth: u32,
) -> Result<DynamicMessage, JsonError> {
    let obj = json
        .as_object()
        .ok_or_else(|| JsonError::TypeMismatch("Struct".into(), "expected object".into()))?;
    let value_desc = pool.lookup_message(".google.protobuf.Value");
    let mut entries = Vec::with_capacity(obj.len());
    for (k, v) in obj {
        let value_msg = match &value_desc {
            Some(d) => super::json_to_message_impl(pool, d, v, opts, depth + 1)?,
            None => DynamicMessage::new(Arc::new(DescriptorProto::default()), pool.clone()),
        };
        entries.push((
            Value::Scalar(RScalar::String(k.clone())),
            Value::Message(value_msg),
        ));
    }
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), pool.clone());
    msg.set_field(1, Value::Map(entries));
    Ok(msg)
}

fn value_to_json(
    pool: &DescriptorPool,
    msg: &DynamicMessage,
    opts: &JsonOptions,
    depth: u32,
) -> Result<Json, JsonError> {
    if let Some(Value::Enum(_)) = msg.get_field(1) {
        // null_value enum; any value maps to JSON null.
        return Ok(Json::Null);
    }
    if let Some(s) = get_f64(msg, 2) {
        return Ok(Json::Number(
            Number::from_f64(s).ok_or_else(|| JsonError::WellKnown("bad number".into()))?,
        ));
    }
    if let Some(s) = get_str(msg, 3) {
        return Ok(Json::String(s));
    }
    if let Some(b) = get_bool(msg, 4) {
        return Ok(Json::Bool(b));
    }
    if let Some(m) = get_msg(msg, 5) {
        let d = pool
            .lookup_message(".google.protobuf.Struct")
            .ok_or_else(|| JsonError::UnresolvedType(".google.protobuf.Struct".into()))?;
        return super::message_to_json_impl(pool, &d, m, opts, depth + 1);
    }
    if let Some(m) = get_msg(msg, 6) {
        let d = pool
            .lookup_message(".google.protobuf.ListValue")
            .ok_or_else(|| JsonError::UnresolvedType(".google.protobuf.ListValue".into()))?;
        return super::message_to_json_impl(pool, &d, m, opts, depth + 1);
    }
    Ok(Json::Null)
}

fn get_f64(msg: &DynamicMessage, n: i32) -> Option<f64> {
    match msg.get_field(n)? {
        Value::Scalar(RScalar::F64(x)) => Some(*x),
        _ => None,
    }
}

fn value_from_json(
    pool: &DescriptorPool,
    desc: &DescriptorProto,
    json: &Json,
    opts: &JsonOptions,
    depth: u32,
) -> Result<DynamicMessage, JsonError> {
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), pool.clone());
    match json {
        Json::Null => {
            msg.set_field(1, Value::Enum(0)); // NullValue.NULL_VALUE
        }
        Json::Bool(b) => {
            msg.set_field(4, Value::Scalar(RScalar::Bool(*b)));
        }
        Json::Number(n) => {
            msg.set_field(2, Value::Scalar(RScalar::F64(n.as_f64().unwrap_or(0.0))));
        }
        Json::String(s) => {
            msg.set_field(3, Value::Scalar(RScalar::String(s.clone())));
        }
        Json::Object(_) => {
            let d = pool
                .lookup_message(".google.protobuf.Struct")
                .ok_or_else(|| JsonError::UnresolvedType(".google.protobuf.Struct".into()))?;
            let inner = super::json_to_message_impl(pool, &d, json, opts, depth + 1)?;
            msg.set_field(5, Value::Message(inner));
        }
        Json::Array(_arr) => {
            let d = pool
                .lookup_message(".google.protobuf.ListValue")
                .ok_or_else(|| JsonError::UnresolvedType(".google.protobuf.ListValue".into()))?;
            let inner = super::json_to_message_impl(pool, &d, json, opts, depth + 1)?;
            msg.set_field(6, Value::Message(inner));
        }
    }
    Ok(msg)
}

fn list_value_to_json(
    pool: &DescriptorPool,
    msg: &DynamicMessage,
    opts: &JsonOptions,
    depth: u32,
) -> Result<Json, JsonError> {
    let mut out = Vec::new();
    if let Some(Value::List(items)) = msg.get_field(1) {
        let value_desc = pool.lookup_message(".google.protobuf.Value");
        for it in items {
            let json_val = if let (Some(d), Value::Message(m)) = (&value_desc, it) {
                super::message_to_json_impl(pool, d, m, opts, depth + 1)?
            } else {
                Json::Null
            };
            out.push(json_val);
        }
    }
    Ok(Json::Array(out))
}

fn list_value_from_json(
    pool: &DescriptorPool,
    desc: &DescriptorProto,
    json: &Json,
    opts: &JsonOptions,
    depth: u32,
) -> Result<DynamicMessage, JsonError> {
    let arr = json
        .as_array()
        .ok_or_else(|| JsonError::TypeMismatch("ListValue".into(), "expected array".into()))?;
    let value_desc = pool.lookup_message(".google.protobuf.Value");
    let mut items = Vec::with_capacity(arr.len());
    for it in arr {
        let value_msg = match &value_desc {
            Some(d) => super::json_to_message_impl(pool, d, it, opts, depth + 1)?,
            None => DynamicMessage::new(Arc::new(DescriptorProto::default()), pool.clone()),
        };
        items.push(Value::Message(value_msg));
    }
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), pool.clone());
    msg.set_field(1, Value::List(items));
    Ok(msg)
}

// ---------------------------------------------------------------------------
// Any.
// ---------------------------------------------------------------------------

/// True when a type's JSON form inside an `Any` goes under a `"value"` key:
/// well-known types whose JSON is not a plain object, and `Any` itself.
fn any_uses_value_key(pool: &DescriptorPool, desc: &DescriptorProto) -> bool {
    match wkt_name(pool, desc).as_deref() {
        Some("Any" | "Timestamp" | "Duration" | "FieldMask" | "Value" | "ListValue" | "Struct") => {
            true
        }
        Some(name) => is_wrapper(name),
        None => false,
    }
}

fn any_to_json(
    pool: &DescriptorPool,
    msg: &DynamicMessage,
    opts: &JsonOptions,
    depth: u32,
) -> Result<Json, JsonError> {
    let type_url = get_str(msg, 1).unwrap_or_default();
    let value_bytes = match msg.get_field(2) {
        Some(Value::Scalar(RScalar::Bytes(b))) => b.clone(),
        _ => Vec::new(),
    };
    // An Any with no type (the empty message) is written as an empty object.
    if type_url.is_empty() && value_bytes.is_empty() {
        return Ok(Json::Object(Map::new()));
    }
    let mut out = Map::new();
    out.insert("@type".to_string(), Json::String(type_url.clone()));
    if let Some(type_name) = type_url.strip_prefix("type.googleapis.com/") {
        if let Some(desc) = pool.lookup_message(type_name) {
            let mut r = Reader::new(&value_bytes);
            let inner = DynamicMessage::decode(pool, desc.clone(), &mut r)
                .map_err(|e| JsonError::WellKnown(e.to_string()))?;
            let inner_json = super::message_to_json_impl(pool, &desc, &inner, opts, depth + 1)?;
            if any_uses_value_key(pool, &desc) {
                out.insert("value".to_string(), inner_json);
            } else if let Json::Object(inner_obj) = inner_json {
                for (k, v) in inner_obj {
                    out.insert(k, v);
                }
            }
            return Ok(Json::Object(out));
        }
    }
    // Unresolvable type: keep the encoded value as base64.
    out.insert(
        "value".to_string(),
        Json::String(base64::engine::general_purpose::STANDARD.encode(&value_bytes)),
    );
    Ok(Json::Object(out))
}

fn any_from_json(
    pool: &DescriptorPool,
    desc: &DescriptorProto,
    json: &Json,
    opts: &JsonOptions,
    depth: u32,
) -> Result<DynamicMessage, JsonError> {
    let obj = json
        .as_object()
        .ok_or_else(|| JsonError::TypeMismatch("Any".into(), "expected object".into()))?;
    // An empty object is an Any with no type and no value.
    if obj.is_empty() {
        return Ok(DynamicMessage::new(Arc::new(desc.clone()), pool.clone()));
    }
    let type_url = obj
        .get("@type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonError::WellKnown("Any missing @type".into()))?
        .to_string();
    // A type URL needs a '/' separating the host from the message name.
    if type_url.is_empty() || !type_url.contains('/') {
        return Err(JsonError::WellKnown("invalid Any type URL".into()));
    }
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), pool.clone());
    msg.set_field(1, Value::Scalar(RScalar::String(type_url.clone())));
    if let Some(type_name) = type_url.strip_prefix("type.googleapis.com/") {
        if let Some(d) = pool.lookup_message(type_name) {
            // Types with a non-object JSON form (and Any itself) sit under "value".
            let inner_json = if any_uses_value_key(pool, &d) {
                obj.get("value")
                    .cloned()
                    .ok_or_else(|| JsonError::WellKnown("Any missing value".into()))?
            } else {
                let mut inner_obj = obj.clone();
                inner_obj.remove("@type");
                Json::Object(inner_obj)
            };
            let inner = super::json_to_message_impl(pool, &d, &inner_json, opts, depth + 1)?;
            let bytes = inner
                .encode()
                .map_err(|e| JsonError::WellKnown(e.to_string()))?;
            msg.set_field(2, Value::Scalar(RScalar::Bytes(bytes)));
            return Ok(msg);
        }
    }
    // Unresolvable: try a "value" base64 field.
    if let Some(Json::String(b64)) = obj.get("value") {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64)
            .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(b64))
            .map_err(|_| JsonError::WellKnown("bad Any value".into()))?;
        msg.set_field(2, Value::Scalar(RScalar::Bytes(bytes)));
    }
    Ok(msg)
}
