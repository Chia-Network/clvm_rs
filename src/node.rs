use super::allocator::{Allocator, NodePtr, SExp};
use std::fmt;

pub struct Node<'a> {
    pub allocator: &'a Allocator,
    pub node: NodePtr,
}

impl<'a> Node<'a> {
    pub fn new(allocator: &'a Allocator, node: NodePtr) -> Self {
        Node { allocator, node }
    }

    pub fn with_node(&self, node: NodePtr) -> Self {
        Node::new(self.allocator, node)
    }

    pub fn sexp(&self) -> SExp {
        self.allocator.sexp(self.node)
    }

    pub fn atom(&'a self) -> Option<&'a [u8]> {
        match self.sexp() {
            SExp::Atom(_) => Some(self.allocator.atom(self.node)),
            _ => None,
        }
    }

    pub fn pair(&self) -> Option<(Node<'a>, Node<'a>)> {
        match self.sexp() {
            SExp::Pair(left, right) => Some((self.with_node(left), self.with_node(right))),
            _ => None,
        }
    }

    pub fn nullp(&self) -> bool {
        match self.sexp() {
            SExp::Atom(a) => a.is_empty(),
            _ => false,
        }
    }

    pub fn arg_count_is(&self, mut count: usize) -> bool {
        let mut ptr: Self = self.clone();
        loop {
            if count == 0 {
                return ptr.nullp();
            }
            match ptr.sexp() {
                SExp::Pair(_, new_ptr) => {
                    ptr = ptr.with_node(new_ptr).clone();
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

    pub fn as_bool(&self) -> bool {
        match self.atom() {
            Some(v0) => !v0.is_empty(),
            _ => true,
        }
    }

    pub fn from_bool(&self, b: bool) -> Self {
        if b {
            self.one()
        } else {
            self.null()
        }
    }
}

impl<'a> fmt::Debug for Node<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.sexp() {
            SExp::Pair(l, r) => f
                .debug_tuple("")
                .field(&self.with_node(l))
                .field(&self.with_node(r))
                .finish(),
            SExp::Atom(a) => self.allocator.buf(&a).fmt(f),
        }
    }
}

impl<'a> Clone for Node<'a> {
    fn clone(&self) -> Self {
        self.with_node(self.node)
    }
}

impl<'a> IntoIterator for &Node<'a> {
    type Item = Node<'a>;
    type IntoIter = Node<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.clone()
    }
}

impl<'a> Iterator for Node<'a> {
    type Item = Node<'a>;

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
