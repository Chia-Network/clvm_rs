#![no_main]

mod make_tree;

use clvmr::serde::{node_from_bytes_backrefs, node_to_bytes, Serializer};
use clvmr::{Allocator, NodePtr, SExp};
use make_tree::make_tree_limits;

use libfuzzer_sys::fuzz_target;

enum TreeOp {
    SExp(NodePtr),
    Cons,
}

// returns the new root (with a sentinel) as well as the sub-tree under the
// sentinel. This function splits the tree (specified as root) into two parts,
// at the specified node index (node_idx). The sentinel node is expected to be
// a unique NodePtr to replace with node and node_idx with. This lets us
// serialize the tree in two steps, testing the incremental serializer.
// If the specified node_idx is not in the tree, None is returned.
fn insert_sentinel(
    a: &mut Allocator,
    root: NodePtr,
    mut node_idx: i32,
    sentinel: NodePtr,
) -> Option<(NodePtr, NodePtr)> {
    // since CLVM trees are immutable, we have to make a copy of the first part
    // we only need to copy the pairs, since the atoms are immutable
    let mut copy = Vec::new();
    let mut ops = vec![TreeOp::SExp(root)];
    let mut subtree: Option<NodePtr> = None;

    while let Some(op) = ops.pop() {
        match op {
            TreeOp::SExp(node) => {
                if node_idx == 0 {
                    // this is the sentinel node, where we split the tree. We're
                    // replacing it with the sentinel node and remembering the
                    // sub tree that goes here to return as the second return
                    // value.
                    copy.push(sentinel);
                    subtree = Some(node);
                    node_idx -= 1;
                    continue;
                }
                node_idx -= 1;
                match a.sexp(node) {
                    SExp::Atom => {
                        copy.push(node);
                    }
                    SExp::Pair(left, right) => {
                        ops.push(TreeOp::Cons);
                        ops.push(TreeOp::SExp(left));
                        ops.push(TreeOp::SExp(right));
                    }
                }
            }
            TreeOp::Cons => {
                let left = copy.pop().unwrap();
                let right = copy.pop().unwrap();
                copy.push(a.new_pair(left, right).unwrap());
            }
        }
    }

    // node_idx was too big, there aren't that many nodes in the tree
    if node_idx >= 0 {
        return None;
    }

    assert!(subtree.is_some());
    assert!(copy.len() == 1);
    Some((copy[0], subtree.unwrap()))
}

// we ensure that serializing a structure in two steps results in a valid form
// as well as that it correctly represents the tree.
fuzz_target!(|data: &[u8]| {
    let mut unstructured = arbitrary::Unstructured::new(data);
    let mut allocator = Allocator::new();

    // since we copy the tree, we must limit the number of pairs created, to not
    // exceed the limit of the Allocator
    let program = make_tree_limits(&mut allocator, &mut unstructured, 10_000_000, 10_000_000);

    // this just needs to be a unique NodePtr, that won't appear in the tree
    let sentinel = allocator.new_pair(NodePtr::NIL, NodePtr::NIL).unwrap();

    let checkpoint = allocator.checkpoint();
    // count up intil we've used every node as the sentinel/cut-point
    let mut node_idx = 0;

    // try to put the sentinel in all positions, to get full coverage
    while let Some((first_step, second_step)) =
        insert_sentinel(&mut allocator, program, node_idx, sentinel)
    {
        node_idx += 1;

        let mut ser = Serializer::new(Some(sentinel));
        let (done, _) = ser.add(&allocator, first_step).unwrap();
        assert!(!done);
        let (done, _) = ser.add(&allocator, second_step).unwrap();
        assert!(done);

        // now, make sure that we deserialize to the exact same structure, by
        // comparing the uncompressed form
        let roundtrip = node_from_bytes_backrefs(&mut allocator, ser.get_ref()).unwrap();
        let b1 = node_to_bytes(&allocator, roundtrip).unwrap();

        let b2 = node_to_bytes(&allocator, program).unwrap();

        assert_eq!(&hex::encode(&b1), &hex::encode(&b2));

        // free the memory used by the last iteration from the allocator,
        // otherwise we'll exceed the Allocator limits eventually
        allocator.restore_checkpoint(&checkpoint);
    }
});
