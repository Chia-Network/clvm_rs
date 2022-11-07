use std::io;
use std::io::{Cursor, ErrorKind};

use crate::allocator::{NodePtr, SExp};
use crate::node::Node;

const CONS_BOX_MARKER: u8 = 0xff;

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
                f.write_all(&[CONS_BOX_MARKER])?;
                values.push(right);
                values.push(left);
            }
        }
    }
    Ok(())
}

pub fn node_to_bytes(node: &Node) -> io::Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());

    node_to_stream(node, &mut buffer)?;
    let vec = buffer.into_inner();
    Ok(vec)
}
