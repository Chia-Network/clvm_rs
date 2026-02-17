use clvmr::{Allocator, NodePtr, SExp};
use std::collections::HashSet;

/// compare two CLVM trees. Returns true if they are identical, false otherwise
pub fn node_eq(allocator: &Allocator, lhs: NodePtr, rhs: NodePtr) -> bool {
    let mut stack = vec![(lhs, rhs)];
    let mut visited = HashSet::<NodePtr>::new();

    while let Some((l, r)) = stack.pop() {
        match (allocator.sexp(l), allocator.sexp(r)) {
            (SExp::Pair(ll, lr), SExp::Pair(rl, rr)) => {
                if !visited.insert(l) {
                    continue;
                }
                stack.push((lr, rr));
                stack.push((ll, rl));
            }
            (SExp::Atom, SExp::Atom) => {
                if !allocator.atom_eq(l, r) {
                    return false;
                }
            }
            _ => {
                return false;
            }
        }
    }
    true
}

/// Compare two CLVM trees that may belong to different allocators.
/// Returns true if they are structurally identical, false otherwise.
pub fn node_eq_two(
    lhs_allocator: &Allocator,
    lhs: NodePtr,
    rhs_allocator: &Allocator,
    rhs: NodePtr,
) -> bool {
    let mut stack = vec![(lhs, rhs)];

    while let Some((l, r)) = stack.pop() {
        match (lhs_allocator.sexp(l), rhs_allocator.sexp(r)) {
            (SExp::Pair(ll, lr), SExp::Pair(rl, rr)) => {
                stack.push((lr, rr));
                stack.push((ll, rl));
            }
            (SExp::Atom, SExp::Atom) => {
                if lhs_allocator.atom(l).as_ref() != rhs_allocator.atom(r).as_ref() {
                    return false;
                }
            }
            _ => {
                return false;
            }
        }
    }
    true
}
