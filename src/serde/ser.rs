use std::io;
use std::io::Cursor;

use super::write_atom::write_atom;
use crate::allocator::{NodePtr, SExp};
use crate::node::Node;

const CONS_BOX_MARKER: u8 = 0xff;

/// serialize a node
pub fn node_to_stream<W: io::Write>(node: &Node, f: &mut W) -> io::Result<()> {
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
