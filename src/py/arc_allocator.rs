use super::py_node::{PyNode, PySExp};
use crate::allocator::{Allocator, SExp};
use crate::reduction::EvalErr;

use std::sync::Arc;

use aovec::Aovec;

use lazy_static::*;

use pyo3::prelude::*;

#[pyclass(subclass, unsendable)]
pub struct ArcAllocator {
    vec: Aovec<Arc<[u8]>>,
}

static NULL_BYTES: [u8; 0] = [];
static ONE_BYTES: [u8; 1] = [1];

lazy_static! {
    static ref NULL: Arc<[u8]> = Arc::new(NULL_BYTES);
    static ref ONE: Arc<[u8]> = Arc::new(ONE_BYTES);
}

impl Default for ArcAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl ArcAllocator {
    pub fn new() -> Self {
        ArcAllocator {
            vec: Aovec::new(16),
        }
    }

    pub fn blob(&self, v: &str) -> PyNode {
        Arc::new(PySExp::Atom(Vec::from(v).into())).into()
    }
}

impl Allocator for ArcAllocator {
    type Ptr = PyNode;

    fn new_atom(&self, v: &[u8]) -> PyNode {
        Arc::new(PySExp::Atom(Vec::from(v).into())).into()
    }

    fn new_pair(&self, first: &PyNode, rest: &PyNode) -> PyNode {
        let inner_node: Arc<PySExp> = Arc::new(PySExp::Pair(
            Arc::new(first.clone()),
            Arc::new(rest.clone()),
        ));
        inner_node.into()
    }

    fn sexp(&self, node: &PyNode) -> SExp<PyNode> {
        match node.sexp() {
            PySExp::Atom(a) => {
                let b: Arc<[u8]> = a.clone();
                let index = self.vec.push(b);
                let atom = self.vec.get(index).unwrap();
                SExp::Atom(&atom)
            }
            PySExp::Pair(left, right) => SExp::Pair(left.into(), right.into()),
        }
    }

    fn null(&self) -> PyNode {
        let a = NULL.clone();
        Arc::new(PySExp::Atom(a)).into()
    }

    fn one(&self) -> PyNode {
        let a = ONE.clone();
        Arc::new(PySExp::Atom(a)).into()
    }
}

impl ArcAllocator {
    pub fn err<T>(&self, node: &PyNode, msg: &str) -> Result<T, EvalErr<PyNode>> {
        Err(EvalErr(node.clone(), msg.into()))
    }
}
