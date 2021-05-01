use super::arc_allocator::{ArcAllocator, ArcSExp};
use crate::allocator::{Allocator, SExp};
use crate::reduction::EvalErr;
use std::cell::RefCell;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

#[pyclass(subclass, unsendable)]
#[derive(Clone)]
pub struct PyNode {
    node: ArcSExp,
    pyobj: RefCell<Option<PyObject>>,
}

impl std::convert::From<EvalErr<ArcSExp>> for pyo3::PyErr {
    fn from(v: EvalErr<ArcSExp>) -> Self {
        PyValueError::new_err(v.0)
    }
}

fn extract_atom(allocator: &mut ArcAllocator, obj: &PyAny) -> PyResult<PyNode> {
    let py_bytes: &PyBytes = obj.extract()?;
    let r: &[u8] = obj.extract()?;
    let inner_node = allocator.new_atom(r)?;
    let py_node = PyNode::new_cached(inner_node, Some(py_bytes.into()));
    Ok(py_node)
}

fn extract_node<'a>(_allocator: &ArcAllocator, obj: &'a PyAny) -> PyResult<PyRef<'a, PyNode>> {
    let ps: &PyCell<PyNode> = obj.downcast()?;
    let node: PyRef<'a, PyNode> = ps.try_borrow()?;
    Ok(node)
}

fn extract_tuple(allocator: &mut ArcAllocator, obj: &PyAny) -> PyResult<PyNode> {
    let v: &PyTuple = obj.downcast()?;
    if v.len() != 2 {
        return Err(PyValueError::new_err("SExp tuples must be size 2"));
    }
    let i0: &PyAny = v.get_item(0);
    let i1: &PyAny = v.get_item(1);
    let left: PyRef<PyNode> = extract_node(allocator, i0)?;
    let right: PyRef<PyNode> = extract_node(allocator, i1)?;
    let left: &PyNode = &left;
    let right: &PyNode = &right;
    let left: ArcSExp = left.into();
    let right: ArcSExp = right.into();
    let node: ArcSExp = allocator.new_pair(left, right)?;
    let py_node = PyNode::new_cached(node, Some(obj.into()));

    Ok(py_node)
}

impl From<&ArcSExp> for PyNode {
    fn from(item: &ArcSExp) -> Self {
        item.clone().into()
    }
}

impl From<ArcSExp> for PyNode {
    fn from(item: ArcSExp) -> Self {
        Self::new(item)
    }
}

impl<'source> FromPyObject<'source> for ArcSExp {
    fn extract(obj: &'source PyAny) -> PyResult<Self> {
        let py_node: PyNode = obj.extract()?;
        Ok(py_node.into())
    }
}

impl ToPyObject for ArcSExp {
    fn to_object(&self, py: Python<'_>) -> PyObject {
        let pynode: PyNode = self.into();
        let pynode: &PyCell<PyNode> = PyCell::new(py, pynode).unwrap();
        let pa: &PyAny = pynode;
        pa.to_object(py)
    }
}

#[pymethods]
impl PyNode {
    #[new]
    pub fn py_new(obj: &PyAny) -> PyResult<Self> {
        let mut allocator = ArcAllocator::new();
        let node: Self = {
            let n = extract_atom(&mut allocator, obj);
            if let Ok(r) = n {
                r
            } else {
                extract_tuple(&mut allocator, obj)?
            }
        };
        Ok(node)
    }

    #[getter(pair)]
    pub fn pair(&self, py: Python) -> PyResult<Option<PyObject>> {
        match ArcAllocator::new().sexp(&self.node) {
            SExp::Pair(p1, p2) => {
                {
                    let mut borrowed_pair = self.pyobj.borrow_mut();
                    if borrowed_pair.is_none() {
                        let r1 = PyCell::new(py, Self::new(p1))?;
                        let r2 = PyCell::new(py, Self::new(p2))?;
                        let v: &PyTuple = PyTuple::new(py, &[r1, r2]);
                        let v: PyObject = v.into();
                        *borrowed_pair = Some(v);
                    }
                };
                Ok(self.pyobj.borrow().clone())
            }
            _ => Ok(None),
        }
    }

    pub fn _pair(&self) -> Option<(Self, Self)> {
        match ArcAllocator::new().sexp(&self.node) {
            SExp::Pair(p1, p2) => Some((p1.into(), p2.into())),
            _ => None,
        }
    }

    #[getter(atom)]
    pub fn atom(&self, py: Python) -> Option<PyObject> {
        let alloc = ArcAllocator::new();
        match alloc.sexp(&self.node) {
            SExp::Atom(atom) => {
                {
                    let mut borrowed_bytes = self.pyobj.borrow_mut();
                    if borrowed_bytes.is_none() {
                        let b: &PyBytes = PyBytes::new(py, alloc.buf(&atom));
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
    pub const fn new(node: ArcSExp) -> Self {
        Self::new_cached(node, None)
    }

    pub const fn new_cached(node: ArcSExp, py_val: Option<PyObject>) -> Self {
        Self {
            node,
            pyobj: RefCell::new(py_val),
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

impl IntoPy<PyObject> for ArcSExp {
    fn into_py(self, py: Python<'_>) -> PyObject {
        let pynode: PyNode = self.into();
        let pynode: &PyCell<PyNode> = PyCell::new(py, pynode).unwrap();
        pynode.to_object(py)
    }
}
