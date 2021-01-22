use std::io::Cursor;
use std::io::Read;
use std::io::Write;

use crate::allocator::{Allocator, SExp};
use crate::node::Node;

const MAX_SINGLE_BYTE: u8 = 0x7f;
const CONS_BOX_MARKER: u8 = 0xff;

fn encode_size(f: &mut dyn Write, size: usize) -> std::io::Result<()> {
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
    }
    Ok(())
}

pub fn node_to_stream<T: Allocator>(node: &Node<T>, f: &mut dyn Write) -> std::io::Result<()> {
    let mut values: Vec<T::Ptr> = vec![node.node.clone()];
    let a = node.allocator;
    while !values.is_empty() {
        let n = a.sexp(&values.pop().unwrap());
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
                        encode_size(f, size)?;
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

fn decode_size(f: &mut dyn Read, initial_b: u8) -> std::io::Result<usize> {
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
    let mut v: usize = 0;
    for b in size_blob.iter() {
        v <<= 8;
        v += *b as usize;
    }
    let bytes_to_read = v;
    Ok(bytes_to_read)
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
                } else if b[0] <= MAX_SINGLE_BYTE {
                    values.push(allocator.new_atom(&b));
                } else {
                    let blob_size = decode_size(f, b[0])?;
                    let mut blob: Vec<u8> = vec![0; blob_size];
                    f.read_exact(&mut blob)?;
                    values.push(allocator.new_atom(&blob));
                }
            }
            ParseOp::Cons => {
                // cons
                let v2 = values.pop();
                let v1 = values.pop();
                values.push(allocator.new_pair(&v1.unwrap(), &v2.unwrap()));
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
