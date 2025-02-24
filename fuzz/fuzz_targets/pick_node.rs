use clvmr::{Allocator, NodePtr, SExp};
use std::collections::HashSet;

pub fn pick_node(a: &Allocator, root: NodePtr, mut node_idx: i32) -> NodePtr {
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
