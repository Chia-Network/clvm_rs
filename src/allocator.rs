/**
 * An `Allocator` owns clvm objects, and has references to them via objects
 * of type `T`. The objects must live until the allocator disappears.
 *
 */

pub enum SExp<T, B> {
    Atom(B),
    Pair(T, T),
}

pub trait Allocator {
    type Ptr: Clone;
    type AtomBuf: Clone;

    fn new_atom(&mut self, v: &[u8]) -> Self::Ptr;
    fn new_pair(&mut self, first: Self::Ptr, rest: Self::Ptr) -> Self::Ptr;

    // create a new atom whose value is the given slice of the specified atom
    fn new_substr(&mut self, node: Self::Ptr, start: u32, end: u32) -> Self::Ptr;

    // The lifetime here is a bit special because IntAllocator and ArcAllocator
    // have slightly different requirements. With IntAllocator, all buffers are
    // owned by the allocator, with ArcAllocator all buffers have shared
    // ownership by ArcAllocator::Ptr objects. So the returned buffer here
    // depends on both
    fn atom<'a>(&'a self, node: &'a Self::Ptr) -> &'a [u8];
    fn buf<'a>(&'a self, node: &'a Self::AtomBuf) -> &'a [u8];
    fn sexp(&self, node: &Self::Ptr) -> SExp<Self::Ptr, Self::AtomBuf>;
    fn null(&self) -> Self::Ptr;
    fn one(&self) -> Self::Ptr;
}
