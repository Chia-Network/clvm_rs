use super::allocator::{Allocator, SExp};

pub struct Node<'a, T> {
    pub allocator: &'a dyn Allocator<T>,
    pub node: T,
}

impl<'a, T> Node<'a, T> {
    pub fn new(allocator: &'a dyn Allocator<T>, node: T) -> Self {
        Node { allocator, node }
    }

    pub fn blob_u8(&self, v: &[u8]) -> Self {
        self.with_node(self.allocator.blob_u8(v))
    }

    pub fn with_node(&self, node: T) -> Self {
        Node::new(self.allocator, node)
    }

    pub fn sexp(&self) -> SExp<T> {
        self.allocator.sexp(&self.node)
    }

    pub fn atom(&self) -> Option<&[u8]> {
        match self.sexp() {
            SExp::Atom(a) => Some(a),
            _ => None,
        }
    }

    pub fn pair(&self) -> Option<(Node<'a, T>, Node<'a, T>)> {
        match self.sexp() {
            SExp::Pair(left, right) => Some((self.with_node(left), self.with_node(right))),
            _ => None,
        }
    }

    pub fn make_clone(&self) -> Self {
        self.with_node(self.allocator.make_clone(&self.node))
    }
}

impl<'a, T> IntoIterator for &Node<'a, T> {
    type Item = Node<'a, T>;

    type IntoIter = Node<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.make_clone()
    }
}

impl<'a, T> Iterator for Node<'a, T> {
    type Item = Node<'a, T>;

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
