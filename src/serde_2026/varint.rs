//! Variable-length integer encoding for the 2026 serialization format.

use std::io::{Read, Write};

use crate::error::{EvalErr, Result};

/// Write a signed integer to `w` using variable-length encoding (varint).
///
/// Format: [leading 1s][0 separator][two's complement value]
/// - Single byte (0 leading 1s): 0[7-bit two's complement] → range [-64, 63]
/// - Two bytes (1 leading 1):   10[14-bit two's complement] → range [-8192, 8191]
/// - Three bytes (2 leading 1s): 110[21-bit two's complement] → range [-1048576, 1048575]
/// - etc.
pub fn write_varint<W: Write>(w: &mut W, value: i64) -> std::io::Result<()> {
    // Find the smallest encoding size that can represent the value
    for leading_ones in 0..8 {
        let total_value_bits = 7 + 7 * leading_ones;

        // Calculate the range this encoding can represent (two's complement)
        let min_value = -(1i64 << (total_value_bits - 1));
        let max_value = (1i64 << (total_value_bits - 1)) - 1;

        // Check if value fits in this encoding
        if value < min_value || value > max_value {
            continue; // Need more bytes
        }

        // Convert value to unsigned representation (two's complement with total_value_bits)
        let unsigned_value = if value < 0 {
            (value + (1i64 << total_value_bits)) as u64
        } else {
            value as u64
        };

        // Build the encoding
        // First byte: [leading_ones * '1'][0][bits_in_first_byte bits of value]
        let first_byte = if leading_ones > 0 {
            ((1u8 << leading_ones) - 1) << (8 - leading_ones)
        } else {
            0
        };

        // Extract the high bits for the first byte (big-endian order)
        let high_bits = (unsigned_value >> (leading_ones * 8)) as u8;
        let first_byte = first_byte | high_bits;

        w.write_all(&[first_byte])?;
        for i in (0..leading_ones).rev() {
            let byte_val = (unsigned_value >> (i * 8)) as u8;
            w.write_all(&[byte_val])?;
        }

        return Ok(());
    }

    panic!("Value too large to encode: {}", value);
}

/// Encode a signed integer to bytes using variable-length encoding (varint).
#[allow(dead_code)]
pub fn encode_varint(value: i64) -> Vec<u8> {
    let mut buf = Vec::new();
    write_varint(&mut buf, value).unwrap();
    buf
}

/// Decode a signed integer from a byte stream using variable-length encoding.
pub fn decode_varint<R: Read>(r: &mut R) -> Result<i64> {
    let mut first_byte_buf = [0u8; 1];
    r.read_exact(&mut first_byte_buf)
        .map_err(|_| EvalErr::SerializationError)?;
    let first_byte = first_byte_buf[0];

    // Count leading ones using leading_zeros (faster than loop)
    let leading_ones = (!first_byte).leading_zeros() as usize;

    // Reject invalid prefix: 8 leading ones (e.g. 0xFF) is not a valid varint encoding
    if leading_ones >= 8 {
        return Err(EvalErr::SerializationError);
    }

    // After leading 1s and separator 0, remaining bits are the two's complement value
    let bits_in_first_byte = 7 - leading_ones;
    let total_value_bits = 7 + 7 * leading_ones;

    // Extract value bits from first byte (the high bits of the value)
    let value_mask = (1u8 << bits_in_first_byte) - 1;
    let mut unsigned_value = (first_byte & value_mask) as u64;

    // Read additional bytes using fixed-size buffer (max 7 extra bytes)
    if leading_ones > 0 {
        let mut extra_bytes = [0u8; 7];
        r.read_exact(&mut extra_bytes[..leading_ones])
            .map_err(|_| EvalErr::SerializationError)?;

        for &byte in &extra_bytes[..leading_ones] {
            unsigned_value = (unsigned_value << 8) | (byte as u64);
        }
    }

    // Convert from two's complement to signed
    let sign_bit = 1u64 << (total_value_bits - 1);
    if unsigned_value >= sign_bit {
        // Negative value: subtract 2^total_value_bits
        Ok(unsigned_value as i64 - (1i64 << total_value_bits))
    } else {
        Ok(unsigned_value as i64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_encode_varint() {
        // Test single byte encoding
        assert_eq!(encode_varint(0), vec![0x00]);
        assert_eq!(encode_varint(1), vec![0x01]);
        assert_eq!(encode_varint(-1), vec![0x7f]);
        assert_eq!(encode_varint(63), vec![0x3f]);
        assert_eq!(encode_varint(-64), vec![0x40]);

        // Test two byte encoding
        assert_eq!(encode_varint(64), vec![0x80, 0x40]);
        assert_eq!(encode_varint(8191), vec![0x9f, 0xff]);
        assert_eq!(encode_varint(-65), vec![0xbf, 0xbf]);
        assert_eq!(encode_varint(-8192), vec![0xa0, 0x00]);
    }

    #[test]
    fn test_decode_varint() {
        assert_eq!(decode_varint(&mut Cursor::new(&[0x00][..])).unwrap(), 0);
        assert_eq!(decode_varint(&mut Cursor::new(&[0x01][..])).unwrap(), 1);
        assert_eq!(decode_varint(&mut Cursor::new(&[0x7f][..])).unwrap(), -1);
        assert_eq!(
            decode_varint(&mut Cursor::new(&[0x80, 0x40][..])).unwrap(),
            64
        );
        assert_eq!(
            decode_varint(&mut Cursor::new(&[0x9f, 0xff][..])).unwrap(),
            8191
        );
        assert_eq!(
            decode_varint(&mut Cursor::new(&[0xbf, 0xbf][..])).unwrap(),
            -65
        );
    }

    #[test]
    fn test_decode_rejects_invalid_prefix() {
        // 0xFF (8 leading ones) is invalid - should return error, not panic
        assert!(decode_varint(&mut Cursor::new(&[0xff][..])).is_err());
    }
}
