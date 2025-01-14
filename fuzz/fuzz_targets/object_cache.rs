#![no_main]
mod fuzzing_utils;
mod make_tree;

use clvmr::serde::{node_to_bytes, serialized_length, treehash, ObjectCache};
use clvmr::{Allocator, NodePtr, SExp};
use libfuzzer_sys::fuzz_target;

use fuzzing_utils::{tree_hash, visit_tree};

enum Op {
    Cons,
    Traverse(NodePtr),
}

fn compute_serialized_len(a: &Allocator, n: NodePtr) -> u64 {
    let mut stack: Vec<u64> = vec![];
    let mut op_stack = vec![Op::Traverse(n)];

    while let Some(op) = op_stack.pop() {
        match op {
            Op::Cons => {
                let right = stack.pop().expect("internal error, empty stack");
                let left = stack.pop().expect("internal error, empty stack");
                stack.push(1 + left + right);
            }
            Op::Traverse(n) => match a.sexp(n) {
                SExp::Pair(left, right) => {
                    op_stack.push(Op::Cons);
                    op_stack.push(Op::Traverse(left));
                    op_stack.push(Op::Traverse(right));
                }
                SExp::Atom => {
                    let ser_len = node_to_bytes(a, n)
                        .expect("internal error, failed to serialize")
                        .len() as u64;
                    stack.push(ser_len);
                }
            },
        }
    }
    assert_eq!(stack.len(), 1);
    *stack.last().expect("internal error, empty stack")
}

fuzz_target!(|data: &[u8]| {
    let mut unstructured = arbitrary::Unstructured::new(data);
    let mut allocator = Allocator::new();
    let program = make_tree::make_tree(&mut allocator, &mut unstructured);

    let mut hash_cache = ObjectCache::new(treehash);
    let mut length_cache = ObjectCache::new(serialized_length);
    visit_tree(&allocator, program, |a, node| {
        let expect_hash = tree_hash(a, node);
        let expect_len = compute_serialized_len(a, node);
        let computed_hash = hash_cache.get_or_calculate(a, &node, None).unwrap();
        let computed_len = length_cache.get_or_calculate(a, &node, None).unwrap();
        assert_eq!(computed_hash, &expect_hash);
        assert_eq!(computed_len, &expect_len);
    });
});
