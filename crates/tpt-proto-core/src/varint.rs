//! Varint and zigzag encoding/decoding.
//!
//! See the protobuf encoding guide: base-128 varints with the most
//! significant bit used as a continuation flag.

/// The maximum number of bytes a 64-bit varint may occupy.
pub const MAX_VARINT_LEN: usize = 10;

/// Encode a `u64` as a base-128 varint, appending to `buf`.
#[inline]
pub fn encode_varint(mut value: u64, buf: &mut Vec<u8>) {
    while value >= 0x80 {
        buf.push((value as u8) | 0x80);
        value >>= 7;
    }
    buf.push(value as u8);
}

/// Decode a base-128 varint from a slice starting at `bytes[start]`.
///
/// Returns the value and the number of bytes consumed.
pub fn decode_varint(bytes: &[u8], start: usize) -> crate::Result<(u64, usize)> {
    let mut result: u64 = 0;
    let mut shift: u32 = 0;
    let mut i = start;
    loop {
        let byte = *bytes.get(i).ok_or(crate::Error::UnexpectedEof)?;
        if i - start == MAX_VARINT_LEN {
            // 10th byte: only the lowest bit is allowed (bits 63..=69 don't exist).
            if byte & 0x7f > 1 {
                return Err(crate::Error::VarintTooLong);
            }
        }
        if shift >= 64 {
            return Err(crate::Error::VarintTooLong);
        }
        result |= ((byte & 0x7f) as u64) << shift;
        i += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    Ok((result, i - start))
}

/// Encode a signed 32-bit integer as zigzag (so small negatives stay small).
#[inline]
pub const fn encode_zigzag32(value: i32) -> u32 {
    ((value << 1) ^ (value >> 31)) as u32
}

/// Decode a zigzag-encoded `u32` back into an `i32`.
#[inline]
pub const fn decode_zigzag32(value: u32) -> i32 {
    ((value >> 1) as i32) ^ -((value & 1) as i32)
}

/// Encode a signed 64-bit integer as zigzag.
#[inline]
pub const fn encode_zigzag64(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

/// Decode a zigzag-encoded `u64` back into an `i64`.
#[inline]
pub const fn decode_zigzag64(value: u64) -> i64 {
    ((value >> 1) as i64) ^ -((value & 1) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip() {
        for v in [0u64, 1, 127, 128, 300, u32::MAX as u64, u64::MAX] {
            let mut buf = Vec::new();
            encode_varint(v, &mut buf);
            let (decoded, n) = decode_varint(&buf, 0).unwrap();
            assert_eq!(decoded, v);
            assert_eq!(n, buf.len());
        }
    }

    #[test]
    fn varint_len() {
        let mut buf = Vec::new();
        encode_varint(1, &mut buf);
        assert_eq!(buf.len(), 1);
        let mut buf = Vec::new();
        encode_varint(300, &mut buf);
        assert_eq!(buf.len(), 2);
    }

    #[test]
    fn zigzag_roundtrip() {
        for v in [i32::MIN, -1, 0, 1, i32::MAX] {
            assert_eq!(decode_zigzag32(encode_zigzag32(v)), v);
        }
        for v in [i64::MIN, -1, 0, 1, i64::MAX] {
            assert_eq!(decode_zigzag64(encode_zigzag64(v)), v);
        }
    }
}
