//! Unknown field storage and passthrough.

use crate::wire_type::{Tag, WireType};
use crate::Reader;
use crate::Writer;
use std::collections::BTreeMap;

/// A single unknown value, retaining enough information to re-encode it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnknownValue {
    /// A varint (wire type 0).
    Varint(u64),
    /// A 32-bit fixed value (wire type 5).
    Fixed32(u32),
    /// A 64-bit fixed value (wire type 1).
    Fixed64(u64),
    /// A length-delimited blob (wire type 2).
    LengthDelimited(Vec<u8>),
}

/// Storage for fields not recognized by the schema.
///
/// Preserves bytes so messages can be forwarded/merged losslessly. Iteration
/// order is by ascending field number (which is also the deterministic order).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnknownFieldSet {
    fields: BTreeMap<u32, Vec<UnknownValue>>,
}

impl UnknownFieldSet {
    /// Create an empty set.
    pub fn new() -> Self {
        UnknownFieldSet::default()
    }

    /// Whether the set contains no values.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Number of distinct field numbers stored.
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Approximate byte size of all stored values (for limit accounting).
    pub fn encoded_len(&self) -> usize {
        let mut n = 0usize;
        for (field, values) in &self.fields {
            for v in values {
                n += tag_len(*field) + value_len(v);
            }
        }
        n
    }

    /// Insert a value under `field_number`.
    pub fn insert(&mut self, field_number: u32, value: UnknownValue) {
        self.fields.entry(field_number).or_default().push(value);
    }

    /// Read and store one value for `tag` from `reader`.
    pub fn store(&mut self, tag: Tag, reader: &mut Reader) -> crate::Result<()> {
        let value = match tag.wire_type {
            WireType::Varint => UnknownValue::Varint(reader.read_varint()?),
            WireType::Fixed32 => UnknownValue::Fixed32(reader.read_fixed32()?),
            WireType::Fixed64 => UnknownValue::Fixed64(reader.read_fixed64()?),
            WireType::LengthDelimited => {
                UnknownValue::LengthDelimited(reader.read_length_delimited()?.to_vec())
            }
            WireType::StartGroup | WireType::EndGroup => {
                // Groups are not preserved as unknown fields in this runtime.
                return Err(crate::Error::UnexpectedWireType {
                    found: tag.wire_type.as_u8(),
                });
            }
        };
        self.insert(tag.field_number, value);
        reader.limits().check_unknown(self.encoded_len())?;
        Ok(())
    }

    /// Re-encode all unknown fields (in ascending field-number order).
    pub fn encode(&self, w: &mut Writer) {
        for (field, values) in &self.fields {
            for v in values {
                write_value(w, *field, v);
            }
        }
    }

    /// Iterate over `(field_number, &value)` pairs in ascending field order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &UnknownValue)> {
        self.fields
            .iter()
            .flat_map(|(f, vs)| vs.iter().map(move |v| (*f, v)))
    }

    /// Merge another set into this one.
    pub fn merge_from(&mut self, other: &UnknownFieldSet) {
        for (f, vs) in &other.fields {
            self.fields.entry(*f).or_default().extend(vs.iter().cloned());
        }
    }
}

fn tag_len(field: u32) -> usize {
    let mut raw = ((field << 3) | WireType::Varint.as_u8() as u32) as u64;
    let mut n = 1;
    while raw >= 0x80 {
        raw >>= 7;
        n += 1;
    }
    n
}

fn value_len(v: &UnknownValue) -> usize {
    match v {
        UnknownValue::Varint(x) => varint_len(*x),
        UnknownValue::Fixed32(_) => 4,
        UnknownValue::Fixed64(_) => 8,
        UnknownValue::LengthDelimited(b) => varint_len(b.len() as u64) + b.len(),
    }
}

fn varint_len(mut v: u64) -> usize {
    let mut n = 1;
    while v >= 0x80 {
        v >>= 7;
        n += 1;
    }
    n
}

fn write_value(w: &mut Writer, field: u32, v: &UnknownValue) {
    match v {
        UnknownValue::Varint(x) => {
            w.write_tag(field, WireType::Varint);
            w.write_varint(*x);
        }
        UnknownValue::Fixed32(x) => {
            w.write_tag(field, WireType::Fixed32);
            w.write_fixed32(*x);
        }
        UnknownValue::Fixed64(x) => {
            w.write_tag(field, WireType::Fixed64);
            w.write_fixed64(*x);
        }
        UnknownValue::LengthDelimited(b) => {
            w.write_tag(field, WireType::LengthDelimited);
            w.write_length_delimited(b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_and_reencode() {
        let mut w = Writer::new();
        crate::scalar::encode_int32(&mut w, 5, -3);
        crate::scalar::encode_string(&mut w, 9, "x");
        let mut r = Reader::new(w.buf());
        let mut set = UnknownFieldSet::new();
        while !r.is_empty() {
            let tag = r.read_tag().unwrap();
            set.store(tag, &mut r).unwrap();
        }
        assert_eq!(set.len(), 2);
        let mut out = Writer::new();
        set.encode(&mut out);
        assert_eq!(out.buf(), w.buf());
    }
}
