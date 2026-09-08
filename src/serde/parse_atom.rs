use std::io::{Cursor, Read, Seek, SeekFrom};

use crate::allocator::{Allocator, NodePtr};

use crate::error::{EvalErr, Result};

const MAX_SINGLE_BYTE: u8 = 0x7f;

/// decode the length prefix for an atom, returning both the offset to the start
/// of the atom and the full length of the atom.
/// Atoms whose value fit in 7 bits don't have a length prefix, so those should
/// be handled specially and never passed to this function.
///
/// Also rejects non-canonical encodings: over-long length prefixes, and a
/// length-prefixed 1-byte atom whose value is `< 0x80` (those must use the
/// single-byte form). When `atom_size == 1`, the first payload byte is peeked
/// (read then seek'd back) so the stream still points at the atom contents.
pub fn decode_size_with_offset<R: Read + Seek>(f: &mut R, initial_b: u8) -> Result<(u8, u64)> {
    debug_assert!((initial_b & 0x80) != 0);
    if (initial_b & 0x80) == 0 {
        return Err(EvalErr::InternalError(
            NodePtr::NIL,
            "Error Initializing Encoding".to_string(),
        ));
    }

    let atom_start_offset = initial_b.leading_ones() as usize;
    if atom_start_offset >= 8 {
        return Err(EvalErr::SerializationError);
    }
    let bit_mask: u8 = 0xff >> atom_start_offset;
    let b = initial_b & bit_mask;
    let mut stack_allocation = [0_u8; 8];
    let size_blob = &mut stack_allocation[..atom_start_offset];
    size_blob[0] = b;
    if atom_start_offset > 1 {
        let remaining_buffer = &mut size_blob[1..];
        f.read_exact(remaining_buffer)?;
    }
    // need to convert size_blob to an int
    let mut atom_size: u64 = 0;
    if size_blob.len() > 6 {
        return Err(EvalErr::SerializationError);
    }
    for b in size_blob {
        atom_size <<= 8;
        atom_size += *b as u64;
    }
    if atom_size >= 0x400000000 {
        return Err(EvalErr::SerializationError);
    }
    // reject over-long length prefixes (must use the shortest encoding)
    let min_size: u64 = match atom_start_offset {
        1 => 0, // 0x80 (empty) through 0xbf (63 bytes)
        2 => 1 << 6,
        3 => 1 << (5 + 8),
        4 => 1 << (4 + 8 + 8),
        5 => 1 << (3 + 8 + 8 + 8),
        6 => 1 << (2 + 8 + 8 + 8 + 8),
        _ => return Err(EvalErr::SerializationError),
    };
    if atom_size < min_size {
        return Err(EvalErr::NonCanonicalSerialization);
    }
    // 1-byte values < 0x80 must use the single-byte form (no length prefix)
    if atom_size == 1 {
        let mut first = [0_u8];
        f.read_exact(&mut first)?;
        f.seek(SeekFrom::Current(-1))?;
        if first[0] < 0x80 {
            return Err(EvalErr::NonCanonicalSerialization);
        }
    }
    Ok((atom_start_offset as u8, atom_size))
}

pub fn decode_size<R: Read + Seek>(f: &mut R, initial_b: u8) -> Result<u64> {
    decode_size_with_offset(f, initial_b).map(|v| v.1)
}

/// parse an atom from the stream and return a pointer to it
/// the first byte has already been read
fn parse_atom_ptr<'a>(f: &'a mut Cursor<&[u8]>, first_byte: u8) -> Result<&'a [u8]> {
    let blob = if first_byte <= MAX_SINGLE_BYTE {
        let pos = f.position() as usize;
        &f.get_ref()[pos - 1..pos]
    } else {
        let blob_size = decode_size(f, first_byte)?;
        let pos = f.position() as usize;
        if f.get_ref().len() < pos + blob_size as usize {
            return Err(EvalErr::SerializationError);
        }
        f.seek(SeekFrom::Current(blob_size as i64))?;
        &f.get_ref()[pos..(pos + blob_size as usize)]
    };
    Ok(blob)
}

/// parse an atom from the stream into the allocator
/// At this point, the first byte has already been read to ensure it's
/// not a special code like `CONS_BOX_MARKER` = 0xff, so it must be
/// passed in too
pub fn parse_atom(
    allocator: &mut Allocator,
    first_byte: u8,
    f: &mut Cursor<&[u8]>,
) -> Result<NodePtr> {
    if first_byte == 0x01 {
        Ok(allocator.one())
    } else if first_byte == 0x80 {
        Ok(allocator.nil())
    } else {
        let blob = parse_atom_ptr(f, first_byte)?;
        Ok(allocator.new_atom(blob)?)
    }
}

/// parse an atom from the stream and return a pointer to it
pub fn parse_path<'a>(f: &'a mut Cursor<&[u8]>) -> Result<&'a [u8]> {
    let mut buf1: [u8; 1] = [0];
    f.read_exact(&mut buf1)?;
    parse_atom_ptr(f, buf1[0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde::write_atom::write_atom;
    use rstest::rstest;

    #[rstest]
    // single-byte length prefix
    #[case(0b10100000, &[], (1, 0x20))]
    // empty atom (0x80): size 0, no payload to peek
    #[case(0x80, &[], (1, 0))]
    // length-1 atoms with value >= 0x80 (canonical); payload is peeked then restored
    #[case(0x81, &[0x80], (1, 1))]
    #[case(0x81, &[0xff], (1, 1))]
    // two-byte length prefix
    #[case(0b11001111, &[0xaa], (2, 0xfaa))]
    // largest atom that fits in a 5-byte length prefix (canonical)
    #[case(0b11111011, &[0xff, 0xff, 0xff, 0xff], (5, 0x3ffffffff))]
    #[case(0b11011111, &[0], (2, 0x1f00))]
    #[case(0b11101111, &[0, 0], (3, 0xf0000))]
    #[case(0b11110111, &[0, 0, 0], (4, 0x7000000))]
    #[case(0b11111011, &[0, 0, 0, 0], (5, 0x300000000))]
    fn test_decode_size_success(
        #[case] first_b: u8,
        #[case] stream: &[u8],
        #[case] expect: (u8, u64),
    ) {
        let mut stream = Cursor::new(stream);
        let pos_before = stream.position();
        let result = decode_size_with_offset(&mut stream, first_b).expect("expect success");
        assert_eq!(result, expect);
        // length-prefix bytes consumed; payload (if any) left unread
        let prefix_extra = (expect.0 as u64).saturating_sub(1);
        assert_eq!(stream.position(), pos_before + prefix_extra);
    }

    #[rstest]
    // this is an atom length-prefix 0xffffffffffff, or (2^48 - 1).
    // We don't support atoms this large and we should fail before attempting to
    // allocate this much memory
    #[case(0b11111110, &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff], "bad encoding")]
    // this is still too large
    #[case(0b11111100, &[0x4, 0, 0, 0, 0], "bad encoding")]
    // this ensures a fuzzer-found bug doesn't reoccur
    #[case(0b11111100, &[0xff, 0xfe], "bad encoding")]
    // the stream is truncated
    #[case(0b11111100, &[0x4, 0, 0, 0], "bad encoding")]
    // length-1 prefix but missing payload byte
    #[case(0x81, &[], "bad encoding")]
    // atoms are too large
    #[case(0b11111101, &[0, 0, 0, 0, 0], "bad encoding")]
    #[case(0b11111110, &[0x80, 0, 0, 0, 0, 0], "bad encoding")]
    #[case(0b11111111, &[0x80, 0, 0, 0, 0, 0, 0], "bad encoding")]
    // over-long length prefixes
    #[case(0xc0, &[0x00], "non-canonical encoding")]
    #[case(0xc0, &[0x3f], "non-canonical encoding")]
    // over-long encoding of a length-1 atom (must use 0x81, not 2-byte prefix)
    #[case(0xc0, &[0x01], "non-canonical encoding")]
    #[case(0xe0, &[0x00, 0x00], "non-canonical encoding")]
    #[case(0xe0, &[0x1f, 0xff], "non-canonical encoding")]
    // 6-byte prefix for a size that fits in 5 bytes
    #[case(0b11111100, &[0x3, 0xff, 0xff, 0xff, 0xff], "non-canonical encoding")]
    // length-prefixed single-byte atoms (0x00..=0x7f must be bare)
    #[case(0x81, &[0x00], "non-canonical encoding")]
    #[case(0x81, &[0x01], "non-canonical encoding")]
    #[case(0x81, &[0x7f], "non-canonical encoding")]
    fn test_decode_size_failure(#[case] first_b: u8, #[case] stream: &[u8], #[case] expect: &str) {
        let mut stream = Cursor::new(stream);
        assert_eq!(
            decode_size_with_offset(&mut stream, first_b)
                .unwrap_err()
                .to_string(),
            expect
        );
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic]
    fn test_decode_size_panic() {
        let mut stream = Cursor::new(&[0x4, 0, 0, 0]);
        let _ = decode_size_with_offset(&mut stream, 0x7f);
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn test_decode_size_panic() {
        let mut stream = Cursor::new(&[0x4, 0, 0, 0]);
        assert_eq!(
            decode_size_with_offset(&mut stream, 0x7f)
                .unwrap_err()
                .to_string(),
            "Internal Error: Error Initializing Encoding"
        );
    }

    fn check_parse_atom(blob: &[u8], expected_atom: &[u8]) {
        let mut cursor = Cursor::<&[u8]>::new(blob);
        let mut first: [u8; 1] = [0];
        cursor.read_exact(&mut first).unwrap();
        let first = first[0];

        let mut allocator = Allocator::new();
        let atom_node = parse_atom(&mut allocator, first, &mut cursor).unwrap();

        let atom = allocator.atom(atom_node);

        assert_eq!(expected_atom, atom.as_ref());
    }

    fn check_parse_atom_str(blob_hex: &str, expected_atom_hex: &str) {
        let blob = hex::decode(blob_hex).unwrap();
        let expected_atom: &[u8] = &hex::decode(expected_atom_hex).unwrap();
        check_parse_atom(&blob, expected_atom);
    }

    #[test]
    fn test_parse_atom() {
        check_parse_atom_str("80", "");
        // try "00", "01", "02", ..., "7f"
        for idx in 0..128 {
            check_parse_atom(&[idx], &[idx]);
        }

        // check a short atom
        check_parse_atom_str("83666f6f", "666f6f");

        // check long atoms near boundary conditions
        let n = 3;
        let base_lengths = [0, 0x40 - n, 0x2000 - n, 0x100000 - n, 0x08000000 - n];
        let mut atom_vec = vec![];
        for base_length in base_lengths.iter() {
            for size_offset in 0..6 {
                let size = base_length + size_offset;
                atom_vec.resize(size, 0x66);
                let mut buffer: Vec<u8> = vec![];
                let mut cursor = Cursor::new(&mut buffer);
                write_atom(&mut cursor, &atom_vec).unwrap();
            }
        }
    }

    #[test]
    fn test_truncated_parse_atom() {
        // the stream is truncated
        let first = 0b11111100;
        let mut cursor = Cursor::<&[u8]>::new(&[0x4, 0, 0, 0]);
        let mut allocator = Allocator::new();
        let ret = parse_atom(&mut allocator, first, &mut cursor);
        let err = ret.unwrap_err();
        assert_eq!(err.to_string(), "bad encoding".to_string());
    }

    #[rstest]
    #[case("8100")]
    #[case("8101")]
    #[case("817f")]
    fn test_parse_atom_rejects_length_prefixed_small(#[case] input: &str) {
        let blob = hex::decode(input).unwrap();
        let mut cursor = Cursor::<&[u8]>::new(blob.as_slice());
        let mut first = [0_u8; 1];
        cursor.read_exact(&mut first).unwrap();
        let mut allocator = Allocator::new();
        assert_eq!(
            parse_atom(&mut allocator, first[0], &mut cursor).unwrap_err(),
            EvalErr::NonCanonicalSerialization
        );
    }
}
