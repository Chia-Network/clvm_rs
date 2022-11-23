// Serialization with "back-references"

use std::io;
use std::io::Cursor;

use super::object_cache::{serialized_length, treehash, ObjectCache};
use super::read_cache_lookup::ReadCacheLookup;
use super::write_atom::write_atom;
use crate::allocator::{NodePtr, SExp};
use crate::node::Node;

const BACK_REFERENCE: u8 = 0xfe;
const CONS_BOX_MARKER: u8 = 0xff;

#[derive(PartialEq, Eq)]
enum ReadOp {
    Parse,
    Cons,
}

// these test cases were produced by:

// from chia.types.blockchain_format.program import Program
// a = Program.to(...)
// print(bytes(a).hex())
// print(a.get_tree_hash().hex())

pub fn node_to_stream_backrefs<W: io::Write>(node: &Node, f: &mut W) -> io::Result<()> {
    let allocator = node.allocator;
    let mut read_op_stack: Vec<ReadOp> = vec![ReadOp::Parse];
    let mut write_stack: Vec<NodePtr> = vec![node.node];

    let mut read_cache_lookup = ReadCacheLookup::new();

    let mut thc = ObjectCache::new(allocator, treehash);
    let mut slc = ObjectCache::new(allocator, serialized_length);

    while !write_stack.is_empty() {
        let node_to_write = write_stack.pop().expect("write_stack empty");

        let op = read_op_stack.pop();
        assert!(op == Some(ReadOp::Parse));

        let node_serialized_length = *slc
            .get_or_calculate(&node_to_write)
            .expect("couldn't calculate serialized length");
        let node_tree_hash = thc
            .get_or_calculate(&node_to_write)
            .expect("can't get treehash");
        match read_cache_lookup.find_path(node_tree_hash, node_serialized_length) {
            Some(path) => {
                f.write_all(&[BACK_REFERENCE])?;
                write_atom(f, &path)?;
                read_cache_lookup.push(*node_tree_hash);
            }
            None => match allocator.sexp(node_to_write) {
                SExp::Pair(left, right) => {
                    f.write_all(&[CONS_BOX_MARKER])?;
                    write_stack.push(right);
                    write_stack.push(left);
                    read_op_stack.push(ReadOp::Cons);
                    read_op_stack.push(ReadOp::Parse);
                    read_op_stack.push(ReadOp::Parse);
                }
                SExp::Atom(atom_buf) => {
                    let atom = allocator.buf(&atom_buf);
                    write_atom(f, atom)?;
                    read_cache_lookup.push(*node_tree_hash);
                }
            },
        }
        while !read_op_stack.is_empty() && read_op_stack[read_op_stack.len() - 1] == ReadOp::Cons {
            read_op_stack.pop();
            read_cache_lookup.pop2_and_cons();
        }
    }
    Ok(())
}

pub fn node_to_bytes_backrefs(node: &Node) -> io::Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());

    node_to_stream_backrefs(node, &mut buffer)?;
    let vec = buffer.into_inner();
    Ok(vec)
}
