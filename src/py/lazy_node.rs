use crate::allocator::{Allocator, NodePtr, SExp};
use std::rc::Rc;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

#[pyclass(subclass, unsendable)]
#[derive(Clone)]
pub struct LazyNode {
    allocator: Rc<Allocator>,
    node: NodePtr,
    obj: Option<PyObject>,
}

#[pymethods]
impl LazyNode {
    #[getter(pair)]
    pub fn pair(&mut self, py: Python) -> PyResult<Option<PyObject>> {
        match &self.allocator.sexp(self.node) {
            SExp::Pair(_p1, _p2) => {
                self.populate(py)?;
                Ok(self.obj.clone())
            }

            _ => Ok(None),
        }
    }

    #[getter(atom)]
    pub fn atom(&mut self, py: Python) -> PyResult<Option<PyObject>> {
        match &self.allocator.sexp(self.node) {
            SExp::Atom(_atom) => {
                self.populate(py)?;
                Ok(self.obj.clone())
            }
            _ => Ok(None),
        }
    }
}

impl LazyNode {
    pub const fn new(a: Rc<Allocator>, n: NodePtr) -> Self {
        Self {
            allocator: a,
            node: n,
            obj: None,
        }
    }

    pub fn new_cell(py: Python, a: Rc<Allocator>, n: NodePtr) -> PyResult<&PyCell<Self>> {
        PyCell::new(
            py,
            Self {
                allocator: a,
                node: n,
                obj: None,
            },
        )
    }

    pub fn populate(&mut self, py: Python) -> PyResult<()> {
        if self.obj.is_none() {
            let new_obj: PyObject = {
                match &self.allocator.sexp(self.node) {
                    SExp::Pair(p1, p2) => {
                        let r1 = Self::new_cell(py, self.allocator.clone(), *p1)?.to_object(py);
                        let r2 = Self::new_cell(py, self.allocator.clone(), *p2)?.to_object(py);
                        let v: &PyTuple = PyTuple::new(py, &[r1, r2]);
                        v.into()
                    }
                    SExp::Atom(atom) => PyBytes::new(py, self.allocator.buf(atom)).into(),
                }
            };
            self.obj = Some(new_obj);
        }
        Ok(())
    }

    pub fn is_populated(&self) -> bool {
        self.obj.is_some()
    }
}
