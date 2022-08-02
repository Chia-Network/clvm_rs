use std::io;
use std::io::{Cursor, ErrorKind, Read, Seek, SeekFrom};

use crate::allocator::{Allocator, NodePtr, SExp};
use crate::node::Node;

const MAX_SINGLE_BYTE: u8 = 0x7f;
const CONS_BOX_MARKER: u8 = 0xff;

fn bad_encoding() -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, "bad encoding")
}

fn internal_error() -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, "internal error")
}

/// all atoms serialize their contents verbatim. All expect those one-byte atoms
/// from 0x00-0x7f also have a prefix encoding their length. This function
/// writes the correct prefix for an atom of size `size` whose first byte is `atom_0`.
/// If the atom is of size 0, use any placeholder first byte, as it's ignored anyway.

fn write_atom_encoding_prefix_with_size(
    f: &mut dyn io::Write,
    atom_0: u8,
    size: u64,
) -> io::Result<()> {
    if size == 0 {
        f.write_all(&[0x80])
    } else if size == 1 && atom_0 < 0x80 {
        Ok(())
    } else if size < 0x40 {
        f.write_all(&[0x80 | (size as u8)])
    } else if size < 0x2000 {
        f.write_all(&[0xc0 | (size >> 8) as u8, size as u8])
    } else if size < 0x10_0000 {
        f.write_all(&[
            (0xe0 | (size >> 16)) as u8,
            ((size >> 8) & 0xff) as u8,
            ((size) & 0xff) as u8,
        ])
    } else if size < 0x800_0000 {
        f.write_all(&[
            (0xf0 | (size >> 24)) as u8,
            ((size >> 16) & 0xff) as u8,
            ((size >> 8) & 0xff) as u8,
            ((size) & 0xff) as u8,
        ])
    } else if size < 0x4_0000_0000 {
        f.write_all(&[
            (0xf8 | (size >> 32)) as u8,
            ((size >> 24) & 0xff) as u8,
            ((size >> 16) & 0xff) as u8,
            ((size >> 8) & 0xff) as u8,
            ((size) & 0xff) as u8,
        ])
    } else {
        Err(io::Error::new(ErrorKind::InvalidData, "atom too big"))
    }
}

/// serialize an atom
fn write_atom(f: &mut dyn io::Write, atom: &[u8]) -> io::Result<()> {
    let u8_0 = if !atom.is_empty() { atom[0] } else { 0 };
    write_atom_encoding_prefix_with_size(f, u8_0, atom.len() as u64)?;
    f.write_all(atom)
}

/// serialize a node
pub fn node_to_stream(node: &Node, f: &mut dyn io::Write) -> io::Result<()> {
    let mut values: Vec<NodePtr> = vec![node.node];
    let a = node.allocator;
    while !values.is_empty() {
        let v = values.pop().unwrap();
        let n = a.sexp(v);
        match n {
            SExp::Atom(atom_ptr) => {
                let atom = a.buf(&atom_ptr);
                write_atom(f, atom)?;
            }
            SExp::Pair(left, right) => {
                f.write_all(&[CONS_BOX_MARKER as u8])?;
                values.push(right);
                values.push(left);
            }
        }
    }
    Ok(())
}

/// decode the length prefix for an atom. Atoms whose value fit in 7 bits
/// don't have a length prefix, so those should be handled specially and
/// never passed to this function.
fn decode_size(f: &mut dyn io::Read, initial_b: u8) -> io::Result<u64> {
    debug_assert!((initial_b & 0x80) != 0);
    if (initial_b & 0x80) == 0 {
        return Err(internal_error());
    }

    let mut bit_count = 0;
    let mut bit_mask: u8 = 0x80;
    let mut b = initial_b;
    while b & bit_mask != 0 {
        bit_count += 1;
        b &= 0xff ^ bit_mask;
        bit_mask >>= 1;
    }
    let mut size_blob: Vec<u8> = Vec::new();
    size_blob.resize(bit_count, 0);
    size_blob[0] = b;
    if bit_count > 1 {
        let remaining_buffer = &mut size_blob[1..];
        f.read_exact(remaining_buffer)?;
    }
    // need to convert size_blob to an int
    let mut v: u64 = 0;
    if size_blob.len() > 6 {
        return Err(bad_encoding());
    }
    for b in &size_blob {
        v <<= 8;
        v += *b as u64;
    }
    if v >= 0x400000000 {
        return Err(bad_encoding());
    }
    Ok(v)
}

enum ParseOp {
    SExp,
    Cons,
}

/// deserialize a clvm node from a `std::io::Cursor`
pub fn node_from_stream(allocator: &mut Allocator, f: &mut Cursor<&[u8]>) -> io::Result<NodePtr> {
    let mut values: Vec<NodePtr> = Vec::new();
    let mut ops = vec![ParseOp::SExp];

    let mut b = [0; 1];
    loop {
        let op = ops.pop();
        if op.is_none() {
            break;
        }
        match op.unwrap() {
            ParseOp::SExp => {
                f.read_exact(&mut b)?;
                if b[0] == CONS_BOX_MARKER {
                    ops.push(ParseOp::Cons);
                    ops.push(ParseOp::SExp);
                    ops.push(ParseOp::SExp);
                } else if b[0] == 0x01 {
                    values.push(allocator.one());
                } else if b[0] == 0x80 {
                    values.push(allocator.null());
                } else if b[0] <= MAX_SINGLE_BYTE {
                    values.push(allocator.new_atom(&b)?);
                } else {
                    let blob_size = decode_size(f, b[0])?;
                    if (f.get_ref().len() as u64) < blob_size {
                        return Err(bad_encoding());
                    }
                    let mut blob: Vec<u8> = vec![0; blob_size as usize];
                    f.read_exact(&mut blob)?;
                    values.push(allocator.new_atom(&blob)?);
                }
            }
            ParseOp::Cons => {
                // cons
                let v2 = values.pop();
                let v1 = values.pop();
                values.push(allocator.new_pair(v1.unwrap(), v2.unwrap())?);
            }
        }
    }
    Ok(values.pop().unwrap())
}

pub fn node_from_bytes(allocator: &mut Allocator, b: &[u8]) -> io::Result<NodePtr> {
    let mut buffer = Cursor::new(b);
    node_from_stream(allocator, &mut buffer)
}

pub fn node_to_bytes(node: &Node) -> io::Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());

    node_to_stream(node, &mut buffer)?;
    let vec = buffer.into_inner();
    Ok(vec)
}

pub fn serialized_length_from_bytes(b: &[u8]) -> io::Result<u64> {
    let mut f = Cursor::new(b);
    let mut ops = vec![ParseOp::SExp];
    let mut b = [0; 1];
    loop {
        let op = ops.pop();
        if op.is_none() {
            break;
        }
        match op.unwrap() {
            ParseOp::SExp => {
                f.read_exact(&mut b)?;
                if b[0] == CONS_BOX_MARKER {
                    // since all we're doing is to determing the length of the
                    // serialized buffer, we don't need to do anything about
                    // "cons". So we skip pushing it to lower the pressure on
                    // the op stack
                    //ops.push(ParseOp::Cons);
                    ops.push(ParseOp::SExp);
                    ops.push(ParseOp::SExp);
                } else if b[0] == 0x80 || b[0] <= MAX_SINGLE_BYTE {
                    // This one byte we just read was the whole atom.
                    // or the
                    // special case of NIL
                } else {
                    let blob_size = decode_size(&mut f, b[0])?;
                    f.seek(SeekFrom::Current(blob_size as i64))?;
                    if (f.get_ref().len() as u64) < f.position() {
                        return Err(bad_encoding());
                    }
                }
            }
            ParseOp::Cons => {
                // cons. No need to construct any structure here. Just keep
                // going
            }
        }
    }
    Ok(f.position())
}

use crate::sha2::{Digest, Sha256};

fn hash_atom(buf: &[u8]) -> [u8; 32] {
    let mut ctx = Sha256::new();
    ctx.update(&[1_u8]);
    ctx.update(buf);
    ctx.finalize().into()
}

fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut ctx = Sha256::new();
    ctx.update(&[2_u8]);
    ctx.update(left);
    ctx.update(right);
    ctx.finalize().into()
}

// computes the tree-hash of a CLVM structure in serialized form
pub fn tree_hash_from_stream(f: &mut Cursor<&[u8]>) -> io::Result<[u8; 32]> {
    let mut values: Vec<[u8; 32]> = Vec::new();
    let mut ops = vec![ParseOp::SExp];

    let mut b = [0; 1];
    loop {
        let op = ops.pop();
        if op.is_none() {
            break;
        }
        match op.unwrap() {
            ParseOp::SExp => {
                f.read_exact(&mut b)?;
                if b[0] == CONS_BOX_MARKER {
                    ops.push(ParseOp::Cons);
                    ops.push(ParseOp::SExp);
                    ops.push(ParseOp::SExp);
                } else if b[0] == 0x80 {
                    values.push(hash_atom(&[]));
                } else if b[0] <= MAX_SINGLE_BYTE {
                    values.push(hash_atom(&b));
                } else {
                    let blob_size = decode_size(f, b[0])?;
                    let blob = &f.get_ref()[f.position() as usize..];
                    if (blob.len() as u64) < blob_size {
                        return Err(bad_encoding());
                    }
                    f.set_position(f.position() + blob_size);
                    values.push(hash_atom(&blob[..blob_size as usize]));
                }
            }
            ParseOp::Cons => {
                // cons
                let v2 = values.pop();
                let v1 = values.pop();
                values.push(hash_pair(&v1.unwrap(), &v2.unwrap()));
            }
        }
    }
    Ok(values.pop().unwrap())
}

#[test]
fn test_tree_hash_max_single_byte() {
    let mut ctx = Sha256::new();
    ctx.update(&[1_u8]);
    ctx.update(&[0x7f_u8]);
    let mut cursor = Cursor::<&[u8]>::new(&[0x7f_u8]);
    assert_eq!(
        tree_hash_from_stream(&mut cursor).unwrap(),
        ctx.finalize().as_slice()
    );
}

#[test]
fn test_tree_hash_one() {
    let mut ctx = Sha256::new();
    ctx.update(&[1_u8]);
    ctx.update(&[1_u8]);
    let mut cursor = Cursor::<&[u8]>::new(&[1_u8]);
    assert_eq!(
        tree_hash_from_stream(&mut cursor).unwrap(),
        ctx.finalize().as_slice()
    );
}

#[test]
fn test_tree_hash_zero() {
    let mut ctx = Sha256::new();
    ctx.update(&[1_u8]);
    ctx.update(&[0_u8]);
    let mut cursor = Cursor::<&[u8]>::new(&[0_u8]);
    assert_eq!(
        tree_hash_from_stream(&mut cursor).unwrap(),
        ctx.finalize().as_slice()
    );
}

#[test]
fn test_tree_hash_nil() {
    let mut ctx = Sha256::new();
    ctx.update(&[1_u8]);
    let mut cursor = Cursor::<&[u8]>::new(&[0x80_u8]);
    assert_eq!(
        tree_hash_from_stream(&mut cursor).unwrap(),
        ctx.finalize().as_slice()
    );
}

#[test]
fn test_tree_hash_overlong() {
    let mut cursor = Cursor::<&[u8]>::new(&[0x8f, 0xff]);
    let e = tree_hash_from_stream(&mut cursor).unwrap_err();
    assert_eq!(e.kind(), bad_encoding().kind());

    let mut cursor = Cursor::<&[u8]>::new(&[0b11001111, 0xff]);
    let e = tree_hash_from_stream(&mut cursor).unwrap_err();
    assert_eq!(e.kind(), bad_encoding().kind());

    let mut cursor = Cursor::<&[u8]>::new(&[0b11001111, 0xff, 0, 0]);
    let e = tree_hash_from_stream(&mut cursor).unwrap_err();
    assert_eq!(e.kind(), bad_encoding().kind());
}

#[cfg(test)]
use hex::FromHex;

// these test cases were produced by:

// from chia.types.blockchain_format.program import Program
// a = Program.to(...)
// print(bytes(a).hex())
// print(a.get_tree_hash().hex())

#[test]
fn test_tree_hash_list() {
    // this is the list (1 (2 (3 (4 (5 ())))))
    let buf = Vec::from_hex("ff01ff02ff03ff04ff0580").unwrap();
    let mut cursor = Cursor::<&[u8]>::new(&buf);
    assert_eq!(
        tree_hash_from_stream(&mut cursor).unwrap().to_vec(),
        Vec::from_hex("123190dddde51acfc61f48429a879a7b905d1726a52991f7d63349863d06b1b6").unwrap()
    );
}

#[test]
fn test_tree_hash_tree() {
    // this is the tree ((1, 2), (3, 4))
    let buf = Vec::from_hex("ffff0102ff0304").unwrap();
    let mut cursor = Cursor::<&[u8]>::new(&buf);
    assert_eq!(
        tree_hash_from_stream(&mut cursor).unwrap().to_vec(),
        Vec::from_hex("2824018d148bc6aed0847e2c86aaa8a5407b916169f15b12cea31fa932fc4c8d").unwrap()
    );
}

#[test]
fn test_tree_hash_tree_large_atom() {
    // this is the tree ((1, 2), (3, b"foobar"))
    let buf = Vec::from_hex("ffff0102ff0386666f6f626172").unwrap();
    let mut cursor = Cursor::<&[u8]>::new(&buf);
    assert_eq!(
        tree_hash_from_stream(&mut cursor).unwrap().to_vec(),
        Vec::from_hex("b28d5b401bd02b65b7ed93de8e916cfc488738323e568bcca7e032c3a97a12e4").unwrap()
    );
}

#[test]
fn test_serialized_length_from_bytes() {
    assert_eq!(
        serialized_length_from_bytes(&[0x7f, 0x00, 0x00, 0x00]).unwrap(),
        1
    );
    assert_eq!(
        serialized_length_from_bytes(&[0x80, 0x00, 0x00, 0x00]).unwrap(),
        1
    );
    assert_eq!(
        serialized_length_from_bytes(&[0xff, 0x00, 0x00, 0x00]).unwrap(),
        3
    );
    assert_eq!(
        serialized_length_from_bytes(&[0xff, 0x01, 0xff, 0x80, 0x80, 0x00]).unwrap(),
        5
    );

    let e = serialized_length_from_bytes(&[0x8f, 0xff]).unwrap_err();
    assert_eq!(e.kind(), bad_encoding().kind());
    assert_eq!(e.to_string(), "bad encoding");

    let e = serialized_length_from_bytes(&[0b11001111, 0xff]).unwrap_err();
    assert_eq!(e.kind(), bad_encoding().kind());
    assert_eq!(e.to_string(), "bad encoding");

    let e = serialized_length_from_bytes(&[0b11001111, 0xff, 0, 0]).unwrap_err();
    assert_eq!(e.kind(), bad_encoding().kind());
    assert_eq!(e.to_string(), "bad encoding");

    assert_eq!(
        serialized_length_from_bytes(&[0x8f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap(),
        16
    );
}

#[test]
fn test_write_atom_encoding_prefix_with_size() {
    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0, 0).is_ok());
    assert_eq!(buf, vec![0x80]);

    for v in 0..0x7f {
        let mut buf = Vec::<u8>::new();
        assert!(write_atom_encoding_prefix_with_size(&mut buf, v, 1).is_ok());
        assert_eq!(buf, vec![]);
    }

    for v in 0x80..0xff {
        let mut buf = Vec::<u8>::new();
        assert!(write_atom_encoding_prefix_with_size(&mut buf, v, 1).is_ok());
        assert_eq!(buf, vec![0x81]);
    }

    for size in 0x1_u8..0x3f_u8 {
        let mut buf = Vec::<u8>::new();
        assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, size as u64).is_ok());
        assert_eq!(buf, vec![0x80 + size]);
    }

    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0b111111).is_ok());
    assert_eq!(buf, vec![0b10111111]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0b1000000).is_ok());
    assert_eq!(buf, vec![0b11000000, 0b1000000]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0xfffff).is_ok());
    assert_eq!(buf, vec![0b11101111, 0xff, 0xff]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0xffffff).is_ok());
    assert_eq!(buf, vec![0b11110000, 0xff, 0xff, 0xff]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0xffffffff).is_ok());
    assert_eq!(buf, vec![0b11111000, 0xff, 0xff, 0xff, 0xff]);

    // this is the largest possible atom size
    let mut buf = Vec::<u8>::new();
    assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0x3ffffffff).is_ok());
    assert_eq!(buf, vec![0b11111011, 0xff, 0xff, 0xff, 0xff]);

    // this is too large
    let mut buf = Vec::<u8>::new();
    assert!(!write_atom_encoding_prefix_with_size(&mut buf, 0xaa, 0x400000000).is_ok());

    for (size, expected_prefix) in [
        (0x1, vec![0x81]),
        (0x2, vec![0x82]),
        (0x3f, vec![0xbf]),
        (0x40, vec![0xc0, 0x40]),
        (0x1fff, vec![0xdf, 0xff]),
        (0x2000, vec![0xe0, 0x20, 0x00]),
        (0xf_ffff, vec![0xef, 0xff, 0xff]),
        (0x10_0000, vec![0xf0, 0x10, 0x00, 0x00]),
        (0x7ff_ffff, vec![0xf7, 0xff, 0xff, 0xff]),
        (0x800_0000, vec![0xf8, 0x08, 0x00, 0x00, 0x00]),
        (0x3_ffff_ffff, vec![0xfb, 0xff, 0xff, 0xff, 0xff]),
    ] {
        let mut buf = Vec::<u8>::new();
        assert!(write_atom_encoding_prefix_with_size(&mut buf, 0xaa, size).is_ok());
        assert_eq!(buf, expected_prefix);
    }
}

#[test]
fn test_write_atom() {
    let mut buf = Vec::<u8>::new();
    assert!(write_atom(&mut buf, &vec![]).is_ok());
    assert_eq!(buf, vec![0b10000000]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom(&mut buf, &vec![0x00]).is_ok());
    assert_eq!(buf, vec![0b00000000]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom(&mut buf, &vec![0x7f]).is_ok());
    assert_eq!(buf, vec![0x7f]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom(&mut buf, &vec![0x80]).is_ok());
    assert_eq!(buf, vec![0x81, 0x80]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom(&mut buf, &vec![0xff]).is_ok());
    assert_eq!(buf, vec![0x81, 0xff]);

    let mut buf = Vec::<u8>::new();
    assert!(write_atom(&mut buf, &vec![0xaa, 0xbb]).is_ok());
    assert_eq!(buf, vec![0x82, 0xaa, 0xbb]);

    for (size, mut expected_prefix) in [
        (0x1, vec![0x81]),
        (0x2, vec![0x82]),
        (0x3f, vec![0xbf]),
        (0x40, vec![0xc0, 0x40]),
        (0x1fff, vec![0xdf, 0xff]),
        (0x2000, vec![0xe0, 0x20, 0x00]),
        (0xf_ffff, vec![0xef, 0xff, 0xff]),
        (0x10_0000, vec![0xf0, 0x10, 0x00, 0x00]),
        (0x7ff_ffff, vec![0xf7, 0xff, 0xff, 0xff]),
        (0x800_0000, vec![0xf8, 0x08, 0x00, 0x00, 0x00]),
        // the next one represents 17 GB of memory, which it then has to serialize
        // so let's not do it until some time in the future when all machines have
        // 64 GB of memory
        // (0x3_ffff_ffff, vec![0xfb, 0xff, 0xff, 0xff, 0xff]),
    ] {
        let mut buf = Vec::<u8>::new();
        let atom = vec![0xaa; size];
        assert!(write_atom(&mut buf, &atom).is_ok());
        expected_prefix.extend(atom);
        assert_eq!(buf, expected_prefix);
    }
}

#[test]
fn test_decode_size() {
    // single-byte length prefix
    let mut buffer = Cursor::new(&[]);
    assert_eq!(decode_size(&mut buffer, 0x80 | 0x20).unwrap(), 0x20);

    // two-byte length prefix
    let first = 0b11001111;
    let mut buffer = Cursor::new(&[0xaa]);
    assert_eq!(decode_size(&mut buffer, first).unwrap(), 0xfaa);
}

#[test]
fn test_large_decode_size() {
    // this is an atom length-prefix 0xffffffffffff, or (2^48 - 1).
    // We don't support atoms this large and we should fail before attempting to
    // allocate this much memory
    let first = 0b11111110;
    let mut buffer = Cursor::new(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    let ret = decode_size(&mut buffer, first);
    let e = ret.unwrap_err();
    assert_eq!(e.kind(), bad_encoding().kind());
    assert_eq!(e.to_string(), "bad encoding");

    // this is still too large
    let first = 0b11111100;
    let mut buffer = Cursor::new(&[0x4, 0, 0, 0, 0]);
    let ret = decode_size(&mut buffer, first);
    let e = ret.unwrap_err();
    assert_eq!(e.kind(), bad_encoding().kind());
    assert_eq!(e.to_string(), "bad encoding");

    // But this is *just* within what we support
    // Still a very large blob, probably enough for a DoS attack
    let first = 0b11111100;
    let mut buffer = Cursor::new(&[0x3, 0xff, 0xff, 0xff, 0xff]);
    assert_eq!(decode_size(&mut buffer, first).unwrap(), 0x3ffffffff);
}

#[test]
fn test_truncated_decode_size() {
    // the stream is truncated
    let first = 0b11111100;
    let mut buffer = Cursor::new(&[0x4, 0, 0, 0]);
    let ret = decode_size(&mut buffer, first);
    let e = ret.unwrap_err();
    assert_eq!(e.kind(), ErrorKind::UnexpectedEof);
}
