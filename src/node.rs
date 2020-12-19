use super::allocator::{Allocator, SExp};

pub struct Node<'a, T> {
    allocator: &'a dyn Allocator<T>,
    pub node: T,
}

impl<'a, T> Node<'a, T> {
    pub fn new(allocator: &'a dyn Allocator<T>, node: T) -> Self {
        Node { allocator, node }
    }

    pub fn blob_u8(&self, v: &[u8]) -> Self {
        self.with_node(self.allocator.blob_u8(v))
    }

    pub fn from_pair(&self, p1: &Self, p2: &Self) -> Self {
        self.with_node(self.allocator.from_pair(&p1.node, &p2.node))
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

    pub fn nullp(&self) -> bool {
        self.allocator.nullp(&self.node)
    }

    pub fn arg_count_is(&self, mut count: usize) -> bool {
        let mut ptr = self.make_clone();
        loop {
            if count == 0 {
                return ptr.nullp();
            }
            match ptr.sexp() {
                SExp::Pair(_, new_ptr) => {
                    ptr = ptr.with_node(new_ptr);
                }
                _ => return false,
            }
            count -= 1;
        }
    }

    pub fn null(&self) -> Self {
        self.with_node(self.allocator.null())
    }

    pub fn one(&self) -> Self {
        self.with_node(self.allocator.one())
    }
}

impl<'a, T> From<&Node<'a, T>> for &'a dyn Allocator<T> {
    fn from(v: &Node<'a, T>) -> Self {
        v.allocator
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
