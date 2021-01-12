/**
 * An `Allocator` owns clvm objects, and has references to them via objects
 * of type `T`. The objects must live until the allocator disappears.
 *
 */

pub enum SExp<'a, T> {
    Atom(&'a [u8]),
    Pair(T, T),
}

pub trait Allocator<T> {
    fn new_atom(&self, v: &[u8]) -> T;
    fn new_pair(&self, first: &T, rest: &T) -> T;
    fn sexp(&self, node: &T) -> SExp<T>;
    fn make_clone(&self, node: &T) -> T;
    fn null(&self) -> T;
    fn one(&self) -> T;
}
