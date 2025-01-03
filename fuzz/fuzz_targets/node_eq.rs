use clvmr::{Allocator, NodePtr, SExp};

/// compare two CLVM trees. Returns true if they are identical, false otherwise
pub fn node_eq(allocator: &Allocator, s1: NodePtr, s2: NodePtr) -> bool {
    match (allocator.sexp(s1), allocator.sexp(s2)) {
        (SExp::Pair(s1a, s1b), SExp::Pair(s2a, s2b)) => {
            node_eq(allocator, s1a, s2a) && node_eq(allocator, s1b, s2b)
        }
        (SExp::Atom, SExp::Atom) => allocator.atom_eq(s1, s2),
        _ => false,
    }
}
