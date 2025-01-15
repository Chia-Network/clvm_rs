use clvmr::{Allocator, NodePtr, SExp};

/// compare two CLVM trees. Returns true if they are identical, false otherwise
pub fn node_eq(allocator: &Allocator, lhs: NodePtr, rhs: NodePtr) -> bool {
    let mut stack = vec![(lhs, rhs)];

    while let Some((l, r)) = stack.pop() {
        match (allocator.sexp(l), allocator.sexp(r)) {
            (SExp::Pair(ll, lr), SExp::Pair(rl, rr)) => {
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
