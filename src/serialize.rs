use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::io::{Error, ErrorKind};

use crate::allocator::{Allocator, SExp};
use crate::node::Node;

const MAX_SINGLE_BYTE: u8 = 0x7f;
const CONS_BOX_MARKER: u8 = 0xff;

fn bad_encoding() -> std::io::Error {
    Error::new(ErrorKind::InvalidInput, "bad encoding")
}

fn encode_size(f: &mut dyn Write, size: u64) -> std::io::Result<()> {
    if size < 0x40 {
        f.write_all(&[(0x80 | size) as u8])?;
    } else if size < 0x2000 {
        f.write_all(&[(0xc0 | (size >> 8)) as u8, ((size) & 0xff) as u8])?;
    } else if size < 0x10_0000 {
        f.write_all(&[
            (0xe0 | (size >> 16)) as u8,
            ((size >> 8) & 0xff) as u8,
            ((size) & 0xff) as u8,
        ])?;
    } else if size < 0x800_0000 {
        f.write_all(&[
            (0xf0 | (size >> 24)) as u8,
            ((size >> 16) & 0xff) as u8,
            ((size >> 8) & 0xff) as u8,
            ((size) & 0xff) as u8,
        ])?;
    } else if size < 0x4_0000_0000 {
        f.write_all(&[
            (0xf8 | (size >> 32)) as u8,
            ((size >> 24) & 0xff) as u8,
            ((size >> 16) & 0xff) as u8,
            ((size >> 8) & 0xff) as u8,
            ((size) & 0xff) as u8,
        ])?;
    } else {
        panic!("atom size exceeded maximum {}", size);
    }
    Ok(())
}

pub fn node_to_stream<T: Allocator>(node: &Node<T>, f: &mut dyn Write) -> std::io::Result<()> {
    let mut values: Vec<T::Ptr> = vec![node.node.clone()];
    let a = node.allocator;
    while !values.is_empty() {
        let v = values.pop().unwrap();
        let n = a.sexp(&v);
        match n {
            SExp::Atom(atom) => {
                let size = atom.len();
                if size == 0 {
                    f.write_all(&[0x80_u8])?;
                } else {
                    let atom0 = atom[0];
                    if size == 1 && (atom0 <= MAX_SINGLE_BYTE) {
                        f.write_all(&[atom0])?;
                    } else {
                        encode_size(f, size as u64)?;
                        f.write_all(&atom)?;
                    }
                }
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

fn decode_size(f: &mut dyn Read, initial_b: u8) -> std::io::Result<u64> {
    // this function decodes the length prefix for an atom. Atoms whose value
    // fit in 7 bits don't have a length-prefix, so those should never be passed
    // to this function.
    assert!((initial_b & 0x80) != 0);

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
    for b in size_blob.iter() {
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

pub fn node_from_stream<T: Allocator>(allocator: &T, f: &mut dyn Read) -> std::io::Result<T::Ptr> {
    let mut values: Vec<T::Ptr> = Vec::new();
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
                    values.push(allocator.new_atom(&b));
                } else {
                    let blob_size = decode_size(f, b[0])?;
                    let mut blob: Vec<u8> = vec![0; blob_size as usize];
                    f.read_exact(&mut blob)?;
                    values.push(allocator.new_atom(&blob));
                }
            }
            ParseOp::Cons => {
                // cons
                let v2 = values.pop();
                let v1 = values.pop();
                values.push(allocator.new_pair(v1.unwrap(), v2.unwrap()));
            }
        }
    }
    Ok(values.pop().unwrap())
}

pub fn node_from_bytes<T: Allocator>(allocator: &T, b: &[u8]) -> std::io::Result<T::Ptr> {
    let mut buffer = Cursor::new(b);
    node_from_stream(allocator, &mut buffer)
}

pub fn node_to_bytes<T: Allocator>(node: &Node<T>) -> std::io::Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());

    node_to_stream(node, &mut buffer)?;
    let vec = buffer.into_inner();
    Ok(vec)
}

#[test]
fn test_encode_size() {
    let mut buf = Vec::<u8>::new();
    assert!(encode_size(&mut buf, 0b111111).is_ok());
    assert_eq!(buf, vec![0b10111111]);

    let mut buf = Vec::<u8>::new();
    assert!(encode_size(&mut buf, 0b1000000).is_ok());
    assert_eq!(buf, vec![0b11000000, 0b1000000]);

    let mut buf = Vec::<u8>::new();
    assert!(encode_size(&mut buf, 0xfffff).is_ok());
    assert_eq!(buf, vec![0b11101111, 0xff, 0xff]);

    let mut buf = Vec::<u8>::new();
    assert!(encode_size(&mut buf, 0xffffff).is_ok());
    assert_eq!(buf, vec![0b11110000, 0xff, 0xff, 0xff]);

    let mut buf = Vec::<u8>::new();
    assert!(encode_size(&mut buf, 0xffffffff).is_ok());
    assert_eq!(buf, vec![0b11111000, 0xff, 0xff, 0xff, 0xff]);

    // this is the largest possible atom size
    let mut buf = Vec::<u8>::new();
    assert!(encode_size(&mut buf, 0x3ffffffff).is_ok());
    assert_eq!(buf, vec![0b11111011, 0xff, 0xff, 0xff, 0xff]);
}

#[test]
#[should_panic]
fn test_encode_panic() {
    // this is too large
    let mut buf = Vec::<u8>::new();
    assert!(encode_size(&mut buf, 0x400000000).is_ok());
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
    assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof);
}
