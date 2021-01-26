/**
 * An `Allocator` owns clvm objects, and has references to them via objects
 * of type `T`. The objects must live until the allocator disappears.
 *
 */

pub enum SExp<'a, T> {
    Atom(&'a [u8]),
    Pair(T, T),
}

pub trait Allocator {
    type Ptr: Clone;

    fn new_atom(&self, v: &[u8]) -> Self::Ptr;
    fn new_pair(&self, first: &Self::Ptr, rest: &Self::Ptr) -> Self::Ptr;
    fn sexp(&self, node: &Self::Ptr) -> SExp<Self::Ptr>;
    fn null(&self) -> Self::Ptr;
    fn one(&self) -> Self::Ptr;
}
