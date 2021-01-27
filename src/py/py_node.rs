use super::arc_allocator::{ArcAllocator, ArcSExp};
use crate::allocator::{Allocator, SExp};
use std::cell::RefCell;
use std::sync::Arc;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

#[pyclass(subclass, unsendable)]
#[derive(Clone)]
pub struct PyNode {
    node: ArcSExp,
    pyobj: RefCell<Option<PyObject>>,
}

fn extract_atom(_allocator: &ArcAllocator, obj: &PyAny) -> PyResult<PyNode> {
    let py_bytes: &PyBytes = obj.extract()?;
    let r: &[u8] = obj.extract()?;
    let r1: Vec<u8> = r.to_owned();
    let r2: Arc<Vec<u8>> = Arc::new(r1);
    let inner_node: ArcSExp = ArcSExp::Atom(r2);
    let py_node = PyNode {
        node: inner_node,
        pyobj: RefCell::new(Some(py_bytes.into())),
    };
    //println!("py_bytes is {:?}", py_node.bytes);
    Ok(py_node)
}

fn extract_node<'a>(_allocator: &ArcAllocator, obj: &'a PyAny) -> PyResult<PyRef<'a, PyNode>> {
    let ps: &PyCell<PyNode> = obj.downcast()?;
    let node: PyRef<'a, PyNode> = ps.try_borrow()?;
    Ok(node)
}

fn extract_tuple(allocator: &ArcAllocator, obj: &PyAny) -> PyResult<PyNode> {
    let v: &PyTuple = obj.downcast()?;
    if v.len() != 2 {
        return Err(PyValueError::new_err("SExp tuples must be size 2"));
    }
    let i0: &PyAny = v.get_item(0);
    let i1: &PyAny = v.get_item(1);
    let left: PyRef<PyNode> = extract_node(&allocator, i0)?;
    let right: PyRef<PyNode> = extract_node(&allocator, i1)?;
    let left: &PyNode = &left;
    let right: &PyNode = &right;
    let left: ArcSExp = left.into();
    let right: ArcSExp = right.into();
    let node: ArcSExp = allocator.new_pair(&left, &right);
    let py_node = PyNode {
        node,
        pyobj: RefCell::new(Some(obj.into())),
    };

    Ok(py_node)
}

impl From<&ArcSExp> for PyNode {
    fn from(item: &ArcSExp) -> Self {
        item.clone().into()
    }
}

impl From<ArcSExp> for PyNode {
    fn from(item: ArcSExp) -> Self {
        PyNode {
            node: item,
            pyobj: RefCell::new(None),
        }
    }
}

#[pymethods]
impl PyNode {
    #[new]
    pub fn py_new(obj: &PyAny) -> PyResult<Self> {
        let allocator = ArcAllocator::new();
        let node: PyNode = {
            let n = extract_atom(&allocator, obj);
            if let Ok(r) = n {
                r
            } else {
                extract_tuple(&allocator, obj)?
            }
        };
        Ok(node)
    }

    #[getter(pair)]
    pub fn pair(&self) -> Option<(PyNode, PyNode)> {
        self._pair()
    }

    pub fn _pair(&self) -> Option<(PyNode, PyNode)> {
        match ArcAllocator::new().sexp(&self.node) {
            SExp::Pair(p1, p2) => Some((p1.into(), p2.into())),
            _ => None,
        }
    }

    #[getter(atom)]
    pub fn atom(&self, py: Python) -> Option<PyObject> {
        match ArcAllocator::new().sexp(&self.node) {
            SExp::Atom(a) => {
                {
                    let mut borrowed_bytes = self.pyobj.borrow_mut();
                    if borrowed_bytes.is_none() {
                        let b: &PyBytes = PyBytes::new(py, a);
                        let obj: PyObject = b.into();
                        *borrowed_bytes = Some(obj);
                    };
                }
                self.pyobj.borrow().clone()
            }
            _ => None,
        }
    }
}

impl PyNode {
    pub fn new(_allocator: &Arc<ArcAllocator>, node: ArcSExp) -> Self {
        PyNode {
            node,
            pyobj: RefCell::new(None),
        }
    }
}

impl From<&PyNode> for ArcSExp {
    fn from(node: &PyNode) -> Self {
        node.clone().into()
    }
}

impl From<PyNode> for ArcSExp {
    fn from(node: PyNode) -> Self {
        node.node
    }
}
