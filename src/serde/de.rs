use std::io;
use std::io::{Cursor, Read};

use crate::allocator::{Allocator, NodePtr};

use super::parse_atom::parse_atom;

const CONS_BOX_MARKER: u8 = 0xff;

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
    while let Some(op) = ops.pop() {
        match op {
            ParseOp::SExp => {
                f.read_exact(&mut b)?;
                if b[0] == CONS_BOX_MARKER {
                    ops.push(ParseOp::Cons);
                    ops.push(ParseOp::SExp);
                    ops.push(ParseOp::SExp);
                } else {
                    values.push(parse_atom(allocator, b[0], f)?);
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
