#![no_main]

mod make_tree;
mod node_eq;

use clvmr::allocator::{Allocator, NodePtr, SExp};
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::serde::write_atom::write_atom;
use clvmr::serde::ReadCacheLookup;
use clvmr::serde::{serialized_length, treehash, ObjectCache};
use std::io;
use std::io::Cursor;
use std::io::Write;

use node_eq::node_eq;

use libfuzzer_sys::fuzz_target;

const BACK_REFERENCE: u8 = 0xfe;
const CONS_BOX_MARKER: u8 = 0xff;

#[derive(PartialEq, Eq)]
enum ReadOp {
    Parse,
    Cons,
}

// make sure back-references returned by ReadCacheLookup are smaller than the
// node they reference
pub fn compare_back_references(allocator: &Allocator, node: NodePtr) -> io::Result<Vec<u8>> {
    let mut f = Cursor::new(Vec::new());

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

        let result1 = read_cache_lookup.find_path(node_tree_hash, node_serialized_length);
        match result1 {
            Some(path) => {
                f.write_all(&[BACK_REFERENCE])?;
                write_atom(&mut f, &path)?;
                read_cache_lookup.push(*node_tree_hash);
                {
                    // make sure the path is never encoded as more bytes than
                    // the node we're referencing
                    use std::io::Write;
                    let mut temp = Cursor::new(Vec::<u8>::new());
                    temp.write_all(&[BACK_REFERENCE])?;
                    write_atom(&mut temp, &path)?;
                    let temp = temp.into_inner();
                    assert!(temp.len() <= node_serialized_length as usize);
                }
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
                    write_atom(&mut f, atom.as_ref())?;
                    read_cache_lookup.push(*node_tree_hash);
                }
            },
        }
        while let Some(ReadOp::Cons) = read_op_stack.last() {
            read_op_stack.pop();
            read_cache_lookup.pop2_and_cons();
        }
    }
    Ok(f.into_inner())
}

// serializing with the regular compressed serializer should yield the same
// result as using the incremental one (as long as it's in a single add() call).
fuzz_target!(|data: &[u8]| {
    let mut unstructured = arbitrary::Unstructured::new(data);
    let mut allocator = Allocator::new();
    let (program, _) = make_tree::make_tree(&mut allocator, &mut unstructured);

    let b1 = compare_back_references(&allocator, program).unwrap();
    let b2 = node_from_bytes_backrefs(&mut allocator, &b1).unwrap();
    assert!(node_eq(&allocator, b2, program));
});
