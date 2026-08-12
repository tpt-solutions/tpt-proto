//! Wire types and field tags.

/// The protobuf wire types. See the encoding guide.
///
/// Groups (`StartGroup`/`EndGroup`, types 3/4) are retained for legacy
/// compatibility; new schemas should not use them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum WireType {
    /// `int32`, `int64`, `uint32`, `uint64`, `sint32`, `sint64`, `bool`, `enum`.
    Varint = 0,
    /// `fixed64`, `sfixed64`, `double`.
    Fixed64 = 1,
    /// `string`, `bytes`, embedded messages, packed repeated fields.
    LengthDelimited = 2,
    /// Legacy group start.
    StartGroup = 3,
    /// Legacy group end.
    EndGroup = 4,
    /// `fixed32`, `sfixed32`, `float`.
    Fixed32 = 5,
}

impl WireType {
    /// Parse a wire type from its 3-bit payload.
    pub fn from_u8(value: u8) -> Option<WireType> {
        match value {
            0 => Some(WireType::Varint),
            1 => Some(WireType::Fixed64),
            2 => Some(WireType::LengthDelimited),
            3 => Some(WireType::StartGroup),
            4 => Some(WireType::EndGroup),
            5 => Some(WireType::Fixed32),
            _ => None,
        }
    }

    /// The numeric wire-type value.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// A field tag: a field number combined with a wire type.
///
/// Encoded on the wire as `(field_number << 3) | wire_type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tag {
    /// The field number (must be >= 1).
    pub field_number: u32,
    /// The wire type of the field.
    pub wire_type: WireType,
}

impl Tag {
    /// Construct a tag, returning `None` if `field_number` is zero.
    pub fn new(field_number: u32, wire_type: WireType) -> Option<Tag> {
        if field_number == 0 {
            None
        } else {
            Some(Tag {
                field_number,
                wire_type,
            })
        }
    }

    /// Encode the tag to a varint and append it to `buf`.
    pub fn encode(self, buf: &mut Vec<u8>) {
        crate::varint::encode_varint(
            ((self.field_number << 3) | self.wire_type.as_u8() as u32) as u64,
            buf,
        );
    }

    /// Decode a tag from `bytes` at `start`, returning the tag and bytes consumed.
    pub fn decode(bytes: &[u8], start: usize) -> crate::Result<(Tag, usize)> {
        let (raw, n) = crate::varint::decode_varint(bytes, start)?;
        let field_number = (raw >> 3) as u32;
        let wire_value = (raw & 0x7) as u8;
        let wire_type = WireType::from_u8(wire_value)
            .ok_or(crate::Error::InvalidTag(raw as u32))?;
        let tag = Tag::new(field_number, wire_type)
            .ok_or(crate::Error::InvalidTag(field_number))?;
        Ok((tag, n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tag_roundtrip() {
        for (field, wt) in [
            (1u32, WireType::Varint),
            (2, WireType::LengthDelimited),
            (15, WireType::Fixed32),
            (16, WireType::Fixed64),
            (300, WireType::StartGroup),
            (301, WireType::EndGroup),
        ] {
            let tag = Tag::new(field, wt).unwrap();
            let mut buf = Vec::new();
            tag.encode(&mut buf);
            let (decoded, n) = Tag::decode(&buf, 0).unwrap();
            assert_eq!(decoded, tag);
            assert_eq!(n, buf.len());
        }
    }

    #[test]
    fn zero_field_rejected() {
        assert!(Tag::new(0, WireType::Varint).is_none());
        let buf = vec![0u8];
        assert!(Tag::decode(&buf, 0).is_err());
    }
}
