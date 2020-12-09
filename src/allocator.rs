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

pub struct NodeT<'a, T> {
    pub allocator: &'a dyn Allocator<T>,
    pub node: T,
}

impl<'a, T> NodeT<'a, T> {
    pub fn new(allocator: &'a dyn Allocator<T>, node: T) -> Self {
        NodeT { allocator, node }
    }

    pub fn with_node(&self, node: T) -> Self {
        NodeT::new(self.allocator, node)
    }

    pub fn sexp(&self) -> SExp<T> {
        self.allocator.sexp(&self.node)
    }

    pub fn atom(&self) -> Option<Arc<[u8]>> {
        match self.sexp() {
            SExp::Atom(a) => Some(a.clone()),
            _ => None,
        }
    }

    pub fn pair(&self) -> Option<(NodeT<'a, T>, NodeT<'a, T>)> {
        match self.sexp() {
            SExp::Pair(left, right) => Some((self.with_node(left), self.with_node(right))),
            _ => None,
        }
    }
}

impl<'a, T> Iterator for NodeT<'a, T> {
    type Item = NodeT<'a, T>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.pair() {
            Some((first, rest)) => {
                self.node = rest.node;
                Some(first)
            }
            _ => None,
        }
    }
}
