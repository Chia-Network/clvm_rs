#![no_main]
mod fuzzing_utils;
mod make_tree;

use clvmr::serde::{node_to_bytes, serialized_length, treehash, ObjectCache};
use clvmr::{Allocator, NodePtr, SExp};
use fuzzing_utils::tree_hash;
use libfuzzer_sys::fuzz_target;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::collections::HashSet;

enum Op {
    Cons(NodePtr),
    Traverse(NodePtr),
}

fn compute_serialized_len(a: &Allocator, n: NodePtr) -> u64 {
    let mut stack: Vec<u64> = vec![];
    let mut op_stack = vec![Op::Traverse(n)];
    let mut cache = HashMap::<NodePtr, u64>::new();

    while let Some(op) = op_stack.pop() {
        match op {
            Op::Cons(node) => {
                let right = stack.pop().expect("internal error, empty stack");
                let left = stack.pop().expect("internal error, empty stack");
                match cache.entry(node) {
                    Entry::Occupied(e) => stack.push(*e.get()),
                    Entry::Vacant(e) => {
                        e.insert(1 + left + right);
                        stack.push(1 + left + right);
                    }
                }
            }
            Op::Traverse(node) => match cache.entry(node) {
                Entry::Occupied(e) => stack.push(*e.get()),
                Entry::Vacant(e) => match a.sexp(node) {
                    SExp::Pair(left, right) => {
                        op_stack.push(Op::Cons(node));
                        op_stack.push(Op::Traverse(left));
                        op_stack.push(Op::Traverse(right));
                    }
                    SExp::Atom => {
                        let ser_len = node_to_bytes(a, node)
                            .expect("internal error, failed to serialize")
                            .len() as u64;
                        e.insert(ser_len);
                        stack.push(ser_len);
                    }
                },
            },
        }
    }
    assert_eq!(stack.len(), 1);
    *stack.last().expect("internal error, empty stack")
}

fn pick_node(a: &Allocator, root: NodePtr, mut node_idx: i32) -> NodePtr {
    let mut stack = vec![root];
    let mut seen_node = HashSet::<NodePtr>::new();

    while let Some(node) = stack.pop() {
        if node_idx == 0 {
            return node;
        }
        if !seen_node.insert(node) {
            continue;
        }
        node_idx -= 1;
        if let SExp::Pair(left, right) = a.sexp(node) {
            stack.push(left);
            stack.push(right);
        }
    }
    NodePtr::NIL
}

fuzz_target!(|data: &[u8]| {
    let mut unstructured = arbitrary::Unstructured::new(data);
    let mut allocator = Allocator::new();
    let (tree, node_count) =
        make_tree::make_tree_limits(&mut allocator, &mut unstructured, 10_000, true);

    let mut hash_cache = ObjectCache::new(treehash);
    let mut length_cache = ObjectCache::new(serialized_length);

    let node_idx = unstructured.int_in_range(0..=node_count).unwrap_or(5) as i32;

    let node = pick_node(&allocator, tree, node_idx);

    let expect_hash = tree_hash(&allocator, node);
    let expect_len = compute_serialized_len(&allocator, node);
    let computed_hash = hash_cache
        .get_or_calculate(&allocator, &node, None)
        .unwrap();
    let computed_len = length_cache
        .get_or_calculate(&allocator, &node, None)
        .unwrap();
    assert_eq!(computed_hash, &expect_hash);
    assert_eq!(computed_len, &expect_len);
});
