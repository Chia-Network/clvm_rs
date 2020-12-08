pub enum SExp<T, U> {
    Atom(U),
    Pair(T, T),
}

pub trait Allocator<T, U> {
    fn blob_u8(&self, v: &[u8]) -> T;
    fn from_pair(&self, first: &T, rest: &T) -> T;
    fn sexp(&self, node: &T) -> SExp<T, U>;
}
