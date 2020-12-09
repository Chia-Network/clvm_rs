use crate::allocator::{Allocator, SExp};
use crate::node::Node;
use std::sync::Arc;

use lazy_static::*;

use pyo3::prelude::*;

#[pyclass(subclass, unsendable)]
pub struct ArcAllocator {}

lazy_static! {
    static ref NULL: Node = {
        let allocator = ArcAllocator::new();
        allocator.blob_u8(&[])
    };
    static ref ONE: Node = {
        let allocator = ArcAllocator::new();
        allocator.blob_u8(&[1])
    };
}

impl Default for ArcAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl ArcAllocator {
    pub fn new() -> Self {
        ArcAllocator {}
    }

    pub fn blob(&self, v: &str) -> Node {
        Arc::new(SExp::Atom(Vec::from(v).into())).into()
    }
}

impl Allocator<Node> for ArcAllocator {
    fn blob_u8(&self, v: &[u8]) -> Node {
        Arc::new(SExp::Atom(Vec::from(v).into())).into()
    }

    fn from_pair(&self, first: &Node, rest: &Node) -> Node {
        Arc::new(SExp::Pair(first.clone(), rest.clone())).into()
    }

    fn sexp(&self, node: &Node) -> SExp<Node> {
        match node.sexp() {
            SExp::Atom(a) => SExp::Atom(Arc::clone(a)),
            SExp::Pair(left, right) => SExp::Pair(left.clone(), right.clone()),
        }
    }

    fn make_clone(&self, node: &Node) -> Node {
        node.clone()
    }
    fn null(&self) -> Node {
        NULL.clone()
    }
    fn one(&self) -> Node {
        ONE.clone()
    }
}
