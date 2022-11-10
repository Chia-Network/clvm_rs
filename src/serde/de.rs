use std::io;
use std::io::{Cursor, ErrorKind, Read, Seek, SeekFrom};

use crate::allocator::{Allocator, NodePtr};

const MAX_SINGLE_BYTE: u8 = 0x7f;
const CONS_BOX_MARKER: u8 = 0xff;

fn bad_encoding() -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, "bad encoding")
}

fn internal_error() -> io::Error {
    io::Error::new(ErrorKind::InvalidInput, "internal error")
}

/// decode the length prefix for an atom. Atoms whose value fit in 7 bits
/// don't have a length prefix, so those should be handled specially and
/// never passed to this function.
fn decode_size<R: io::Read>(f: &mut R, initial_b: u8) -> io::Result<u64> {
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

#[repr(u8)]
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
                    let pos = f.position() as usize;
                    if f.get_ref().len() < pos + blob_size as usize {
                        return Err(bad_encoding());
                    }
                    let blob = &f.get_ref()[pos..(pos + blob_size as usize)];
                    f.seek(SeekFrom::Current(blob_size as i64))?;
                    values.push(allocator.new_atom(blob)?);
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
    ctx.update([1_u8]);
    ctx.update(buf);
    ctx.finalize().into()
}

fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut ctx = Sha256::new();
    ctx.update([2_u8]);
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
