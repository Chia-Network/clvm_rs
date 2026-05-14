use clvmr::allocator::{Allocator, NodePtr, SExp};
use std::rc::Rc;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

#[pyclass(subclass, unsendable, skip_from_py_object)]
#[derive(Clone)]
pub struct LazyNode {
    allocator: Rc<Allocator>,
    node: NodePtr,
}

#[pymethods]
impl LazyNode {
    #[getter(pair)]
    pub fn pair(&self, py: Python) -> PyResult<Option<Py<PyAny>>> {
        match &self.allocator.sexp(self.node) {
            SExp::Pair(p1, p2) => {
                let r1 = Self::new(self.allocator.clone(), *p1);
                let r2 = Self::new(self.allocator.clone(), *p2);
                let v = PyTuple::new(py, [r1, r2])?;
                Ok(Some(v.unbind().into_any()))
            }
            _ => Ok(None),
        }
    }

    #[getter(atom)]
    pub fn atom(&self, py: Python) -> Option<Py<PyAny>> {
        match &self.allocator.sexp(self.node) {
            SExp::Atom => Some(
                PyBytes::new(py, self.allocator.atom(self.node).as_ref())
                    .unbind()
                    .into_any(),
            ),
            _ => None,
        }
    }
}

impl LazyNode {
    pub const fn new(a: Rc<Allocator>, n: NodePtr) -> Self {
        Self {
            allocator: a,
            node: n,
        }
    }

    // Rust-side serializers need direct access to the backing allocator/node.
    // These are intentionally crate-local; Python only sees the atom/pair view.
    pub fn allocator(&self) -> &Allocator {
        &self.allocator
    }

    pub fn node(&self) -> NodePtr {
        self.node
    }
}
