#![no_main]

mod make_tree;
mod node_eq;

use clvmr::allocator::{Allocator, NodePtr, SExp};
use clvmr::error::{EvalErr, Result};
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::serde::write_atom::write_atom;
use clvmr::serde::ReadCacheLookup;
use clvmr::serde::TreeCache;
use clvmr::serde::{serialized_length, treehash, ObjectCache};
use std::io::Cursor;
use std::io::Write;

use node_eq::node_eq;

use libfuzzer_sys::fuzz_target;

const BACK_REFERENCE: u8 = 0xfe;
const CONS_BOX_MARKER: u8 = 0xff;

#[derive(PartialEq, Eq)]
enum ReadOp {
    Parse,
    Cons(NodePtr),
}

// make sure back-references returned by ReadCacheLookup are smaller than the
// node they reference and compare ReadCacheLookup and ObjectCache against TreeCache
pub fn compare_back_references(allocator: &Allocator, node: NodePtr) -> Result<Vec<u8>> {
    let mut f = Cursor::new(Vec::new());

    let mut read_op_stack: Vec<ReadOp> = vec![ReadOp::Parse];
    let mut write_stack: Vec<NodePtr> = vec![node];

    let mut read_cache_lookup = ReadCacheLookup::new();

    let mut thc = ObjectCache::new(treehash);
    let mut slc = ObjectCache::new(serialized_length);

    let mut tree_cache = TreeCache::new(None);
    tree_cache.update(allocator, node);

    while let Some(node_to_write) = write_stack.pop() {
        let op = read_op_stack.pop();
        assert!(op == Some(ReadOp::Parse));

        let node_serialized_length = *slc
            .get_or_calculate(allocator, &node_to_write, None)
            .expect("couldn't calculate serialized length");
        let node_tree_hash = thc
            .get_or_calculate(allocator, &node_to_write, None)
            .expect("can't get treehash");

        let mut result1 = read_cache_lookup.find_path(node_tree_hash, node_serialized_length);
        let result2 = tree_cache.find_path(node_to_write);
        let points_to_stack = if let Some(ref p1) = result1 {
            // the read_cache_lookup supports finding references to the
            // stack element itself (a node that doesn't exist in the
            // original tree). tree_cache does not, so exempt this from
            // the comparison
            // a path pointing to the stack is one that only has 1-bits
            (p1[0] as u32 + 1).count_ones() == 1 && p1[1..].iter().all(|n| *n == 0xff)
        } else {
            false
        };

        if !points_to_stack {
            match (&result1, &result2) {
                (Some(p1), Some(p2)) => {
                    // sometimes there are multiple paths to the node, and which
                    // one we pick may be somewhat arbitrary, just depending on
                    // the order we visit them. This check is just to make sure
                    // we find paths of equal lengths (i.e. both should be the
                    // shortest path)
                    if p1.len() != p2.len() || p1[0].leading_zeros() != p2[0].leading_zeros() {
                        panic!("inconsistent results, {p1:?} != {p2:?} serialized-length: {node_serialized_length}");
                    }
                }
                (None, None) => {}
                (Some(p1), None) => {
                    panic!("read_cache_lookup: {p1:?}, tree_cache: None serialized-length: {node_serialized_length}");
                }
                (None, Some(p2)) => {
                    panic!("read_cache_lookup: None, tree_cache: {p2:?} serialized-length: {node_serialized_length}");
                }
            };
        } else {
            // in order to keep the ReadCacheLookup in sync with TreeCache
            // pretend the lookup resulted in the same path (or no path)
            result1 = result2;
        }
        match result1 {
            Some(path) => {
                f.write_all(&[BACK_REFERENCE])
                    .map_err(|_| clvmr::error::EvalErr::SerializationError)?;
                write_atom(&mut f, &path)?;
                read_cache_lookup.push(*node_tree_hash);
                tree_cache.push(node_to_write);
                {
                    // make sure the path is never encoded as more bytes than
                    // the node we're referencing
                    use std::io::Write;
                    let mut temp = Cursor::new(Vec::<u8>::new());
                    temp.write_all(&[BACK_REFERENCE])
                        .map_err(|_| EvalErr::SerializationError)?;
                    write_atom(&mut temp, &path)?;
                    let temp = temp.into_inner();
                    assert!(temp.len() <= node_serialized_length as usize);
                }
            }
            None => match allocator.sexp(node_to_write) {
                SExp::Pair(left, right) => {
                    f.write_all(&[CONS_BOX_MARKER])
                        .map_err(|_| EvalErr::SerializationError)?;
                    write_stack.push(right);
                    write_stack.push(left);
                    read_op_stack.push(ReadOp::Cons(node_to_write));
                    read_op_stack.push(ReadOp::Parse);
                    read_op_stack.push(ReadOp::Parse);
                }
                SExp::Atom => {
                    let atom = allocator.atom(node_to_write);
                    write_atom(&mut f, atom.as_ref())?;
                    read_cache_lookup.push(*node_tree_hash);
                    tree_cache.push(node_to_write);
                }
            },
        }
        while let Some(ReadOp::Cons(node)) = read_op_stack.last() {
            tree_cache.pop2_and_cons(*node);
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
