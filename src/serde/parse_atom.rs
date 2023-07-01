use std::io::{Cursor, Read, Result, Seek, SeekFrom};

use crate::allocator::{Allocator, NodePtr};

use super::errors::{bad_encoding, internal_error};

const MAX_SINGLE_BYTE: u8 = 0x7f;

/// decode the length prefix for an atom, returning both the offset to the start
/// of the atom and the full length of the atom.
/// Atoms whose value fit in 7 bits don't have a length prefix, so those should
/// be handled specially and never passed to this function.
pub fn decode_size_with_offset<R: Read>(f: &mut R, initial_b: u8) -> Result<(u8, u64)> {
    debug_assert!((initial_b & 0x80) != 0);
    if (initial_b & 0x80) == 0 {
        return Err(internal_error());
    }

    let mut atom_start_offset = 0;
    let mut bit_mask: u8 = 0x80;
    let mut b = initial_b;
    while b & bit_mask != 0 {
        atom_start_offset += 1;
        b &= 0xff ^ bit_mask;
        bit_mask >>= 1;
    }
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
        return Err(bad_encoding());
    }
    for b in size_blob {
        atom_size <<= 8;
        atom_size += *b as u64;
    }
    if atom_size >= 0x400000000 {
        return Err(bad_encoding());
    }
    Ok((atom_start_offset as u8, atom_size))
}

pub fn decode_size<R: Read>(f: &mut R, initial_b: u8) -> Result<u64> {
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
            return Err(bad_encoding());
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
        Ok(allocator.null())
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
use std::io::ErrorKind;

#[cfg(test)]
use hex;

#[cfg(test)]
use super::write_atom::write_atom;

#[test]
fn test_decode_size() {
    // single-byte length prefix
    let mut buffer = Cursor::new(&[]);
    assert_eq!(
        decode_size_with_offset(&mut buffer, 0x80 | 0x20).unwrap(),
        (1, 0x20)
    );

    // two-byte length prefix
    let first = 0b11001111;
    let mut buffer = Cursor::new(&[0xaa]);
    assert_eq!(
        decode_size_with_offset(&mut buffer, first).unwrap(),
        (2, 0xfaa)
    );
}

#[test]
fn test_large_decode_size() {
    // this is an atom length-prefix 0xffffffffffff, or (2^48 - 1).
    // We don't support atoms this large and we should fail before attempting to
    // allocate this much memory
    let first = 0b11111110;
    let mut buffer = Cursor::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    let ret = decode_size_with_offset(&mut buffer, first);
    let e = ret.unwrap_err();
    assert_eq!(e.kind(), bad_encoding().kind());
    assert_eq!(e.to_string(), "bad encoding");

    // this is still too large
    let first = 0b11111100;
    let mut buffer = Cursor::new(&[0x4, 0, 0, 0, 0]);
    let ret = decode_size_with_offset(&mut buffer, first);
    let e = ret.unwrap_err();
    assert_eq!(e.kind(), bad_encoding().kind());
    assert_eq!(e.to_string(), "bad encoding");

    // But this is *just* within what we support
    // Still a very large blob, probably enough for a DoS attack
    let first = 0b11111100;
    let mut buffer = Cursor::new(&[0x3, 0xff, 0xff, 0xff, 0xff]);
    assert_eq!(
        decode_size_with_offset(&mut buffer, first).unwrap(),
        (6, 0x3ffffffff)
    );

    // this ensures a fuzzer-found bug doesn't reoccur
    let mut buffer = Cursor::new(&[0xff, 0xfe]);
    let ret = decode_size_with_offset(&mut buffer, first);
    let e = ret.unwrap_err();
    assert_eq!(e.kind(), ErrorKind::UnexpectedEof);
    assert_eq!(e.to_string(), "failed to fill whole buffer");
}

#[test]
fn test_truncated_decode_size() {
    // the stream is truncated
    let first = 0b11111100;
    let mut cursor = Cursor::new(&[0x4, 0, 0, 0]);
    let ret = decode_size_with_offset(&mut cursor, first);
    let e = ret.unwrap_err();
    assert_eq!(e.kind(), ErrorKind::UnexpectedEof);
}

#[cfg(test)]
fn check_parse_atom(blob: &[u8], expected_atom: &[u8]) {
    let mut cursor = Cursor::<&[u8]>::new(blob);
    let mut first: [u8; 1] = [0];
    cursor.read_exact(&mut first).unwrap();
    let first = first[0];

    let mut allocator = Allocator::new();
    let atom_node = parse_atom(&mut allocator, first, &mut cursor).unwrap();

    let atom_ptr = allocator.atom(atom_node);

    assert_eq!(expected_atom, atom_ptr);
}

#[cfg(test)]
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
    assert_eq!(err.kind(), ErrorKind::UnexpectedEof);
}
