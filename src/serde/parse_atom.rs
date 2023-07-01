use std::io::{Cursor, Read, Result, Seek, SeekFrom};

use crate::allocator::{Allocator, NodePtr};

use super::errors::{bad_encoding, internal_error};

const MAX_SINGLE_BYTE: u8 = 0x7f;

/// Decode the length prefix for an atom, returning both the offset to the start
/// of the atom and the full length of the atom.
/// Atoms whose value fit in 7 bits don't have a length prefix, so those should
/// be handled specially and never passed to this function.
pub fn decode_size_with_offset<R: Read>(reader: &mut R, initial_byte: u8) -> Result<(u8, u64)> {
    debug_assert!((initial_byte & 0x80) != 0);

    if (initial_byte & 0x80) == 0 {
        return Err(internal_error());
    }

    let mut atom_start_offset = 0;
    let mut bit_mask: u8 = 0x80;
    let mut byte = initial_byte;

    while byte & bit_mask != 0 {
        atom_start_offset += 1;
        byte &= 0xff ^ bit_mask;
        bit_mask >>= 1;
    }

    let mut stack_allocation = [0_u8; 8];
    let size_blob = &mut stack_allocation[..atom_start_offset];
    size_blob[0] = byte;

    if atom_start_offset > 1 {
        let remaining_buffer = &mut size_blob[1..];
        reader.read_exact(remaining_buffer)?;
    }

    // Need to convert size_blob to an int.

    let mut atom_size: u64 = 0;

    if size_blob.len() > 6 {
        return Err(bad_encoding());
    }

    for byte in size_blob {
        atom_size <<= 8;
        atom_size += *byte as u64;
    }

    if atom_size >= 0x400000000 {
        return Err(bad_encoding());
    }

    Ok((atom_start_offset as u8, atom_size))
}

pub fn decode_size<R: Read>(reader: &mut R, initial_byte: u8) -> Result<u64> {
    decode_size_with_offset(reader, initial_byte).map(|value| value.1)
}

/// Parse an atom from the stream and return a pointer to it
/// the first byte has already been read.
fn parse_atom_ptr<'a>(reader: &'a mut Cursor<&[u8]>, first_byte: u8) -> Result<&'a [u8]> {
    let blob = if first_byte <= MAX_SINGLE_BYTE {
        let pos = reader.position() as usize;
        &reader.get_ref()[pos - 1..pos]
    } else {
        let blob_size = decode_size(reader, first_byte)?;
        let pos = reader.position() as usize;
        if reader.get_ref().len() < pos + blob_size as usize {
            return Err(bad_encoding());
        }
        reader.seek(SeekFrom::Current(blob_size as i64))?;
        &reader.get_ref()[pos..(pos + blob_size as usize)]
    };
    Ok(blob)
}

/// Parse an atom from the stream into the allocator
/// At this point, the first byte has already been read to ensure it's
/// not a special code like `CONS_BOX_MARKER` = 0xff, so it must be
/// passed in too.
pub fn parse_atom(
    allocator: &mut Allocator,
    first_byte: u8,
    reader: &mut Cursor<&[u8]>,
) -> Result<NodePtr> {
    if first_byte == 0x01 {
        Ok(allocator.one())
    } else if first_byte == 0x80 {
        Ok(allocator.null())
    } else {
        let blob = parse_atom_ptr(reader, first_byte)?;
        Ok(allocator.new_atom(blob)?)
    }
}

/// Parse an atom from the stream and return a pointer to it.
pub fn parse_path<'a>(reader: &'a mut Cursor<&[u8]>) -> Result<&'a [u8]> {
    let mut byte: [u8; 1] = [0];
    reader.read_exact(&mut byte)?;
    parse_atom_ptr(reader, byte[0])
}

#[cfg(test)]
mod tests {
    use crate::serde::write_atom::write_atom;

    use super::*;

    use std::io::ErrorKind;

    use hex;

    #[test]
    fn test_decode_size() {
        // Single-byte length prefix.
        let mut buffer = Cursor::new(&[]);
        assert_eq!(
            decode_size_with_offset(&mut buffer, 0x80 | 0x20).unwrap(),
            (1, 0x20)
        );

        // Two-byte length prefix.
        let first = 0b11001111;
        let mut buffer = Cursor::new(&[0xaa]);
        assert_eq!(
            decode_size_with_offset(&mut buffer, first).unwrap(),
            (2, 0xfaa)
        );
    }

    #[test]
    fn test_large_decode_size() {
        // This is an atom length-prefix 0xffffffffffff, or (2^48 - 1).
        // We don't support atoms this large and we should fail before attempting to
        // allocate this much memory.
        let first = 0b11111110;
        let mut buffer = Cursor::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
        let error = decode_size_with_offset(&mut buffer, first).unwrap_err();
        assert_eq!(error.kind(), bad_encoding().kind());
        assert_eq!(error.to_string(), "bad encoding");

        // This is still too large.
        let first = 0b11111100;
        let mut buffer = Cursor::new(&[0x4, 0, 0, 0, 0]);
        let error = decode_size_with_offset(&mut buffer, first).unwrap_err();
        assert_eq!(error.kind(), bad_encoding().kind());
        assert_eq!(error.to_string(), "bad encoding");

        // But this is *just* within what we support.
        // Still a very large blob, probably enough for a DoS attack.
        let first = 0b11111100;
        let mut buffer = Cursor::new(&[0x3, 0xff, 0xff, 0xff, 0xff]);
        assert_eq!(
            decode_size_with_offset(&mut buffer, first).unwrap(),
            (6, 0x3ffffffff)
        );

        // This ensures a fuzzer-found bug doesn't reoccur.
        let mut buffer = Cursor::new(&[0xff, 0xfe]);
        let error = decode_size_with_offset(&mut buffer, first).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
        assert_eq!(error.to_string(), "failed to fill whole buffer");
    }

    #[test]
    fn test_truncated_decode_size() {
        // The stream is truncated.
        let first = 0b11111100;
        let mut cursor = Cursor::new(&[0x4, 0, 0, 0]);
        let error = decode_size_with_offset(&mut cursor, first).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
    }

    fn check_parse_atom(blob: &[u8], expected_atom: &[u8]) {
        let mut cursor = Cursor::new(blob);
        let mut first: [u8; 1] = [0];
        cursor.read_exact(&mut first).unwrap();
        let first = first[0];

        let mut allocator = Allocator::new();
        let atom_node = parse_atom(&mut allocator, first, &mut cursor).unwrap();
        let atom_ptr = allocator.atom(atom_node);
        assert_eq!(expected_atom, atom_ptr);
    }

    fn check_parse_atom_str(blob_hex: &str, expected_atom_hex: &str) {
        let blob = hex::decode(blob_hex).unwrap();
        let expected_atom: &[u8] = &hex::decode(expected_atom_hex).unwrap();
        check_parse_atom(&blob, expected_atom);
    }

    #[test]
    fn test_parse_atom() {
        check_parse_atom_str("80", "");

        // Try "00", "01", "02", ..., "7f".
        for i in 0..128 {
            check_parse_atom(&[i], &[i]);
        }

        // Check a short atom.
        check_parse_atom_str("83666f6f", "666f6f");

        // Check long atoms near boundary conditions.
        let value = 3;
        let base_lengths = [
            0,
            0x40 - value,
            0x2000 - value,
            0x100000 - value,
            0x08000000 - value,
        ];
        let mut atom_vec = vec![];
        for base_length in base_lengths.iter() {
            for size_offset in 0..6 {
                let size = base_length + size_offset;
                atom_vec.resize(size, 0x66);
                let mut buffer: Vec<u8> = vec![];
                write_atom(&mut Cursor::new(&mut buffer), &atom_vec).unwrap();
            }
        }
    }

    #[test]
    fn test_truncated_parse_atom() {
        // The stream is truncated.
        let first = 0b11111100;
        let mut cursor = Cursor::<&[u8]>::new(&[0x4, 0, 0, 0]);
        let mut allocator = Allocator::new();
        let error = parse_atom(&mut allocator, first, &mut cursor).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::UnexpectedEof);
    }
}
