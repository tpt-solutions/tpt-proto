//! Well-known-type (WKT) JSON forms (§4.7, §12, §14).
//!
//! Implements the protobuf JSON special encodings for the standard
//! `google.protobuf.*` well-known types: Timestamp, Duration, FieldMask,
//! Any, Struct, Value, ListValue, the wrapper types, and Empty.

use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use base64::Engine;
use serde_json::{Map, Number};
use tpt_proto_core::Reader;
use tpt_proto_descriptor::{DescriptorProto, FieldType};
use tpt_proto_reflect::{DescriptorPool, DynamicMessage, ScalarValue as RScalar, Value};

use super::{JsonError, JsonOptions, Json};

fn wkt_name(pool: &DescriptorPool, desc: &DescriptorProto) -> Option<String> {
    let fqn = pool.full_name_by_value(desc)?;
    let trimmed = fqn.trim_start_matches('.');
    trimmed.strip_prefix("google.protobuf.").map(|s| s.to_string())
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
        _wrapper if is_wrapper(&name) => wrapper_to_json(descriptor, msg)?,
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
        "DoubleValue" | "FloatValue" | "Int64Value" | "Uint64Value" | "Int32Value"
            | "Uint32Value" | "BoolValue" | "StringValue" | "BytesValue"
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
    desc.field.iter().find(|f| f.number == Some(n)).and_then(|f| f.r#type)
}

// ---------------------------------------------------------------------------
// Timestamp.
// ---------------------------------------------------------------------------

fn timestamp_to_json(msg: &DynamicMessage) -> Result<Json, JsonError> {
    let secs = get_i64(msg, 1).unwrap_or(0);
    let nanos = get_i64(msg, 2).unwrap_or(0) as i32;
    let dt = Utc
        .timestamp_opt(secs, nanos.clamp(0, 999_999_999) as u32)
        .single()
        .ok_or_else(|| JsonError::WellKnown("invalid timestamp".into()))?;
    Ok(Json::String(format_timestamp(dt, nanos)))
}

fn format_timestamp(dt: DateTime<Utc>, nanos: i32) -> String {
    let base = dt.format("%Y-%m-%dT%H:%M:%S").to_string();
    if nanos == 0 {
        format!("{base}Z")
    } else {
        let frac = format!("{nanos:09}");
        let trimmed = frac.trim_end_matches('0');
        format!("{base}.{trimmed}Z")
    }
}

fn timestamp_from_json(desc: &DescriptorProto, json: &Json) -> Result<DynamicMessage, JsonError> {
    let s = json.as_str().ok_or_else(|| JsonError::TypeMismatch("Timestamp".into(), "expected string".into()))?;
    let dt = DateTime::parse_from_rfc3339(s)
        .map_err(|e| JsonError::WellKnown(format!("invalid RFC3339: {e}")))?;
    let utc = dt.with_timezone(&Utc);
    let secs = utc.timestamp();
    let nanos = utc.timestamp_subsec_nanos() as i32;
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), DescriptorPool::default());
    msg.set_field(1, Value::Scalar(RScalar::I64(secs)));
    msg.set_field(2, Value::Scalar(RScalar::I64(nanos as i64)));
    Ok(msg)
}

// ---------------------------------------------------------------------------
// Duration.
// ---------------------------------------------------------------------------

fn duration_to_json(msg: &DynamicMessage) -> Result<Json, JsonError> {
    let secs = get_i64(msg, 1).unwrap_or(0);
    let nanos = get_i64(msg, 2).unwrap_or(0) as i32;
    let out = format_duration(secs, nanos);
    Ok(Json::String(out))
}

fn format_duration(secs: i64, nanos: i32) -> String {
    let sign = if secs < 0 || (secs == 0 && nanos < 0) { "-" } else { "" };
    let asecs = secs.unsigned_abs();
    let mut out = format!("{sign}{asecs}");
    if nanos != 0 {
        let frac = format!("{:09}", nanos.unsigned_abs());
        let trimmed = frac.trim_end_matches('0');
        out.push('.');
        out.push_str(trimmed);
    }
    out.push('s');
    out
}

fn duration_from_json(desc: &DescriptorProto, json: &Json) -> Result<DynamicMessage, JsonError> {
    let s = json.as_str().ok_or_else(|| JsonError::TypeMismatch("Duration".into(), "expected string".into()))?;
    let trimmed = s.strip_suffix('s').ok_or_else(|| JsonError::WellKnown("duration missing 's'".into()))?;
    let (sign, body) = if let Some(b) = trimmed.strip_prefix('-') {
        (-1i64, b)
    } else {
        (1i64, trimmed)
    };
    let (secs_str, nanos_str) = match body.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (body, None),
    };
    let secs: i64 = secs_str.parse().map_err(|_| JsonError::WellKnown("invalid duration seconds".into()))?;
    let nanos: i32 = match nanos_str {
        Some(b) => {
            let padded = format!("{:0<9}", b.chars().take(9).collect::<String>());
            padded.parse().map_err(|_| JsonError::WellKnown("invalid duration nanos".into()))?
        }
        None => 0,
    };
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), DescriptorPool::default());
    msg.set_field(1, Value::Scalar(RScalar::I64(sign * secs)));
    msg.set_field(2, Value::Scalar(RScalar::I64(sign * nanos as i64)));
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
    Ok(Json::String(paths.join(",")))
}

fn field_mask_from_json(desc: &DescriptorProto, json: &Json) -> Result<DynamicMessage, JsonError> {
    let s = json.as_str().ok_or_else(|| JsonError::TypeMismatch("FieldMask".into(), "expected string".into()))?;
    let items = if s.is_empty() {
        Vec::new()
    } else {
        s.split(',').map(|p| Value::Scalar(RScalar::String(p.to_string()))).collect()
    };
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), DescriptorPool::default());
    msg.set_field(1, Value::List(items));
    Ok(msg)
}

// ---------------------------------------------------------------------------
// Wrapper types.
// ---------------------------------------------------------------------------

fn wrapper_to_json(desc: &DescriptorProto, msg: &DynamicMessage) -> Result<Json, JsonError> {
    let t = field_type(desc, 1).unwrap_or(FieldType::String);
    let Some(v) = msg.get_field(1) else {
        return Ok(Json::Null);
    };
    match v {
        Value::Scalar(s) => scalar_json(t, s),
        _ => Err(JsonError::TypeMismatch("wrapper".into(), "expected scalar".into())),
    }
}

fn scalar_json(t: FieldType, s: &RScalar) -> Result<Json, JsonError> {
    use RScalar::*;
    Ok(match (t, s) {
        (FieldType::String, String(x)) => Json::String(x.clone()),
        (FieldType::Bytes, Bytes(b)) => Json::String(base64::engine::general_purpose::STANDARD.encode(b)),
        (FieldType::Bool, Bool(b)) => Json::Bool(*b),
        (FieldType::Double, F64(x)) | (FieldType::Float, F64(x)) => Json::Number(
            Number::from_f64(*x).ok_or_else(|| JsonError::WellKnown("bad float".into()))?,
        ),
        (_, I64(x)) => Json::Number(Number::from(*x)),
        (_, U64(x)) => Json::Number(Number::from(*x)),
        _ => return Err(JsonError::TypeMismatch("wrapper".into(), "unsupported".into())),
    })
}

fn wrapper_from_json(desc: &DescriptorProto, json: &Json) -> Result<DynamicMessage, JsonError> {
    let t = field_type(desc, 1).unwrap_or(FieldType::String);
    let s = match (t, json) {
        (FieldType::String, Json::String(s)) => RScalar::String(s.clone()),
        (FieldType::Bytes, Json::String(s)) => RScalar::Bytes(
            base64::engine::general_purpose::STANDARD.decode(s).or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(s)).map_err(|_| JsonError::TypeMismatch("BytesValue".into(), "bad base64".into()))?,
        ),
        (FieldType::Bool, Json::Bool(b)) => RScalar::Bool(*b),
        (FieldType::Double, Json::Number(n)) | (FieldType::Float, Json::Number(n)) => {
            RScalar::F64(n.as_f64().ok_or_else(|| JsonError::TypeMismatch("float".into(), "bad".into()))?)
        }
        (_, Json::Number(n)) => RScalar::I64(n.as_i64().unwrap_or(0)),
        _ => return Err(JsonError::TypeMismatch("wrapper".into(), "mismatch".into())),
    };
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), DescriptorPool::default());
    msg.set_field(1, Value::Scalar(s));
    Ok(msg)
}

// ---------------------------------------------------------------------------
// Struct / Value / ListValue.
// ---------------------------------------------------------------------------

fn struct_to_json(pool: &DescriptorPool, msg: &DynamicMessage, opts: &JsonOptions, depth: u32) -> Result<Json, JsonError> {
    let mut out = Map::new();
    if let Some(Value::Map(entries)) = msg.get_field(1) {
        let value_desc = pool.lookup_message(".google.protobuf.Value");
        for (k, v) in entries {
            let key = match k {
                Value::Scalar(RScalar::String(s)) => s.clone(),
                _ => continue,
            };
            let json_val = if let (Some(d), Value::Message(m)) = (&value_desc, v) {
                super::message_to_json_impl(pool, &d, m, opts, depth + 1)?
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
    let obj = json.as_object().ok_or_else(|| JsonError::TypeMismatch("Struct".into(), "expected object".into()))?;
    let value_desc = pool.lookup_message(".google.protobuf.Value");
    let mut entries = Vec::with_capacity(obj.len());
    for (k, v) in obj {
        let value_msg = match &value_desc {
            Some(d) => super::json_to_message_impl(pool, d, v, opts, depth + 1)?,
            None => DynamicMessage::new(Arc::new(DescriptorProto::default()), pool.clone()),
        };
        entries.push((Value::Scalar(RScalar::String(k.clone())), Value::Message(value_msg)));
    }
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), pool.clone());
    msg.set_field(1, Value::Map(entries));
    Ok(msg)
}

fn value_to_json(pool: &DescriptorPool, msg: &DynamicMessage, opts: &JsonOptions, depth: u32) -> Result<Json, JsonError> {
    if let Some(v) = get_i64(msg, 1) {
        // null_value enum; any value maps to JSON null.
        let _ = v;
        return Ok(Json::Null);
    }
    if let Some(s) = get_f64(msg, 2) {
        return Ok(Json::Number(Number::from_f64(s).ok_or_else(|| JsonError::WellKnown("bad number".into()))?));
    }
    if let Some(s) = get_str(msg, 3) {
        return Ok(Json::String(s));
    }
    if let Some(b) = get_bool(msg, 4) {
        return Ok(Json::Bool(b));
    }
    if let Some(m) = get_msg(msg, 5) {
        let d = pool.lookup_message(".google.protobuf.Struct").ok_or_else(|| JsonError::UnresolvedType(".google.protobuf.Struct".into()))?;
        return super::message_to_json_impl(pool, &d, m, opts, depth + 1);
    }
    if let Some(m) = get_msg(msg, 6) {
        let d = pool.lookup_message(".google.protobuf.ListValue").ok_or_else(|| JsonError::UnresolvedType(".google.protobuf.ListValue".into()))?;
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
            msg.set_field(1, Value::Scalar(RScalar::I64(0)));
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
            let d = pool.lookup_message(".google.protobuf.Struct").ok_or_else(|| JsonError::UnresolvedType(".google.protobuf.Struct".into()))?;
            let inner = super::json_to_message_impl(pool, &d, json, opts, depth + 1)?;
            msg.set_field(5, Value::Message(inner));
        }
        Json::Array(_arr) => {
            let d = pool.lookup_message(".google.protobuf.ListValue").ok_or_else(|| JsonError::UnresolvedType(".google.protobuf.ListValue".into()))?;
            let inner = super::json_to_message_impl(pool, &d, json, opts, depth + 1)?;
            msg.set_field(6, Value::Message(inner));
        }
    }
    Ok(msg)
}

fn list_value_to_json(pool: &DescriptorPool, msg: &DynamicMessage, opts: &JsonOptions, depth: u32) -> Result<Json, JsonError> {
    let mut out = Vec::new();
    if let Some(Value::List(items)) = msg.get_field(1) {
        let value_desc = pool.lookup_message(".google.protobuf.Value");
        for it in items {
            let json_val = if let (Some(d), Value::Message(m)) = (&value_desc, it) {
                super::message_to_json_impl(pool, &d, m, opts, depth + 1)?
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
    let arr = json.as_array().ok_or_else(|| JsonError::TypeMismatch("ListValue".into(), "expected array".into()))?;
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

fn any_to_json(pool: &DescriptorPool, msg: &DynamicMessage, opts: &JsonOptions, depth: u32) -> Result<Json, JsonError> {
    let type_url = get_str(msg, 1).unwrap_or_default();
    let value_bytes = match msg.get_field(2) {
        Some(Value::Scalar(RScalar::Bytes(b))) => b.clone(),
        _ => Vec::new(),
    };
    let mut out = Map::new();
    out.insert("@type".to_string(), Json::String(type_url.clone()));
    if let Some(type_name) = type_url.strip_prefix("type.googleapis.com/") {
        if let Some(desc) = pool.lookup_message(type_name) {
            let mut r = Reader::new(&value_bytes);
            let inner = DynamicMessage::decode(pool, desc.clone(), &mut r)
                .map_err(|e| JsonError::WellKnown(e.to_string()))?;
            if let Json::Object(inner_obj) = super::message_to_json_impl(pool, &desc, &inner, opts, depth + 1)? {
                for (k, v) in inner_obj {
                    out.insert(k, v);
                }
            }
            return Ok(Json::Object(out));
        }
    }
    // Unresolvable type: keep the encoded value as base64.
    out.insert("value".to_string(), Json::String(base64::engine::general_purpose::STANDARD.encode(&value_bytes)));
    Ok(Json::Object(out))
}

fn any_from_json(
    pool: &DescriptorPool,
    desc: &DescriptorProto,
    json: &Json,
    opts: &JsonOptions,
    depth: u32,
) -> Result<DynamicMessage, JsonError> {
    let obj = json.as_object().ok_or_else(|| JsonError::TypeMismatch("Any".into(), "expected object".into()))?;
    let type_url = obj
        .get("@type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonError::WellKnown("Any missing @type".into()))?
        .to_string();
    let mut msg = DynamicMessage::new(Arc::new(desc.clone()), pool.clone());
    msg.set_field(1, Value::Scalar(RScalar::String(type_url.clone())));
    if let Some(type_name) = type_url.strip_prefix("type.googleapis.com/") {
        if let Some(d) = pool.lookup_message(type_name) {
            let mut inner_obj = obj.clone();
            inner_obj.remove("@type");
            let inner_json = Json::Object(inner_obj);
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
        let bytes = base64::engine::general_purpose::STANDARD.decode(b64).or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(b64)).map_err(|_| JsonError::WellKnown("bad Any value".into()))?;
        msg.set_field(2, Value::Scalar(RScalar::Bytes(bytes)));
    }
    Ok(msg)
}
