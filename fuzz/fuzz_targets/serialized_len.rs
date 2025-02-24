use clvmr::serde::node_to_bytes;
use clvmr::{Allocator, NodePtr, SExp};
use std::collections::hash_map::Entry;
use std::collections::HashMap;

enum Op {
    Cons(NodePtr),
    Traverse(NodePtr),
}

pub fn compute_serialized_len(a: &Allocator, n: NodePtr) -> u64 {
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
