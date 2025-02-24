// Serialization with "back-references"

use std::io;
use std::io::Cursor;

use super::object_cache::{serialized_length, treehash, ObjectCache};
use super::read_cache_lookup::ReadCacheLookup;
use super::write_atom::write_atom;
use crate::allocator::{Allocator, NodePtr, SExp};
use crate::serde::ser::LimitedWriter;

const BACK_REFERENCE: u8 = 0xfe;
const CONS_BOX_MARKER: u8 = 0xff;

#[derive(PartialEq, Eq)]
enum ReadOp {
    Parse,
    Cons,
}

pub fn node_to_stream_backrefs<W: io::Write>(
    allocator: &Allocator,
    node: NodePtr,
    f: &mut W,
) -> io::Result<()> {
    let mut read_op_stack: Vec<ReadOp> = vec![ReadOp::Parse];
    let mut write_stack: Vec<NodePtr> = vec![node];

    let mut read_cache_lookup = ReadCacheLookup::new();

    let mut thc = ObjectCache::new(treehash);
    let mut slc = ObjectCache::new(serialized_length);

    while let Some(node_to_write) = write_stack.pop() {
        let op = read_op_stack.pop();
        assert!(op == Some(ReadOp::Parse));

        let node_serialized_length = *slc
            .get_or_calculate(allocator, &node_to_write, None)
            .expect("couldn't calculate serialized length");
        let node_tree_hash = thc
            .get_or_calculate(allocator, &node_to_write, None)
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
                SExp::Atom => {
                    let atom = allocator.atom(node_to_write);
                    write_atom(f, atom.as_ref())?;
                    read_cache_lookup.push(*node_tree_hash);
                }
            },
        }
        while let Some(ReadOp::Cons) = read_op_stack.last() {
            read_op_stack.pop();
            read_cache_lookup.pop2_and_cons();
        }
    }
    Ok(())
}

pub fn node_to_bytes_backrefs_limit(
    a: &Allocator,
    node: NodePtr,
    limit: usize,
) -> io::Result<Vec<u8>> {
    let buffer = Cursor::new(Vec::new());
    let mut writer = LimitedWriter::new(buffer, limit);
    node_to_stream_backrefs(a, node, &mut writer)?;
    let vec = writer.into_inner().into_inner();
    Ok(vec)
}

pub fn node_to_bytes_backrefs(a: &Allocator, node: NodePtr) -> io::Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());
    node_to_stream_backrefs(a, node, &mut buffer)?;
    let vec = buffer.into_inner();
    Ok(vec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde::node_to_bytes_backrefs;

    #[test]
    fn test_serialize_limit() {
        let mut a = Allocator::new();

        let leaf = a.new_atom(&[1, 2, 3, 4, 5]).unwrap();
        let l1 = a.new_pair(leaf, leaf).unwrap();
        let l2 = a.new_pair(l1, l1).unwrap();
        let l3 = a.new_pair(l2, l2).unwrap();

        let expected = &[255, 255, 255, 133, 1, 2, 3, 4, 5, 254, 2, 254, 2, 254, 2];

        assert_eq!(node_to_bytes_backrefs(&a, l3).unwrap(), expected);
        assert_eq!(node_to_bytes_backrefs_limit(&a, l3, 15).unwrap(), expected);
        assert_eq!(
            node_to_bytes_backrefs_limit(&a, l3, 14).unwrap_err().kind(),
            io::ErrorKind::OutOfMemory
        );
    }
}
