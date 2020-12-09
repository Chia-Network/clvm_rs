use std::sync::Arc;

pub enum SExp<T> {
    Atom(Arc<[u8]>),
    Pair(T, T),
}

pub trait Allocator<T> {
    fn blob_u8(&self, v: &[u8]) -> T;
    fn from_pair(&self, first: &T, rest: &T) -> T;
    fn sexp(&self, node: &T) -> SExp<T>;
    fn make_clone(&self, node: &T) -> T;
    fn null(&self) -> T;
    fn one(&self) -> T;
}
