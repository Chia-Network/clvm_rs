use super::py_node::PyNode;
use crate::allocator::{Allocator, SExp};
use crate::reduction::EvalErr;
use std::sync::Arc;

use lazy_static::*;

use pyo3::prelude::*;

#[pyclass(subclass, unsendable)]
pub struct ArcAllocator {}

lazy_static! {
    static ref NULL: PyNode = {
        let allocator = ArcAllocator::new();
        allocator.blob_u8(&[])
    };
    static ref ONE: PyNode = {
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

    pub fn blob(&self, v: &str) -> PyNode {
        Arc::new(SExp::Atom(Vec::from(v).into())).into()
    }
}

impl Allocator<PyNode> for ArcAllocator {
    fn blob_u8(&self, v: &[u8]) -> PyNode {
        Arc::new(SExp::Atom(Vec::from(v).into())).into()
    }

    fn from_pair(&self, first: &PyNode, rest: &PyNode) -> PyNode {
        Arc::new(SExp::Pair(first.clone(), rest.clone())).into()
    }

    fn sexp(&self, node: &PyNode) -> SExp<PyNode> {
        match node.sexp() {
            SExp::Atom(a) => SExp::Atom(Arc::clone(a)),
            SExp::Pair(left, right) => SExp::Pair(left.clone(), right.clone()),
        }
    }

    fn make_clone(&self, node: &PyNode) -> PyNode {
        node.clone()
    }

    fn null(&self) -> PyNode {
        NULL.clone()
    }

    fn one(&self) -> PyNode {
        ONE.clone()
    }
}

impl ArcAllocator {
    pub fn err<T>(&self, node: &PyNode, msg: &str) -> Result<T, EvalErr<PyNode>> {
        Err(EvalErr(self.make_clone(node), msg.into()))
    }
}
