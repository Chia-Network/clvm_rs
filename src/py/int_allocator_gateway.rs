use pyo3::{exceptions::PyBufferError, prelude::*};
//use pyo3::prelude::{PyObject, PyResult, Python};
use pyo3::types::PyTuple;
use pyo3::types::PyBytes;

use crate::allocator::{Allocator, SExp};
use crate::int_allocator::{IntAllocator, IntAtomBuf};
use crate::reduction::EvalErr;

use super::gateway::PythonGateway;

#[pyclass(subclass, unsendable)]
pub struct PyCachingAllocator {
    arena: PyObject, // &PyCell<PyIntAllocator>,
                     // TODO: cache created PyObjects
}

/*
impl Allocator for &PyCachingAllocator {
    type Ptr = i32;
    type AtomBuf = IntAtomBuf;

    fn new_atom(&mut self, v: &[u8]) -> Result<Self::Ptr, EvalErr<Self::Ptr>> {
        self.arena.borrow_mut().arena.new_atom(v)
    }

    fn new_pair(
        &mut self,
        first: Self::Ptr,
        rest: Self::Ptr,
    ) -> Result<Self::Ptr, EvalErr<Self::Ptr>> {
        self.arena.borrow_mut().arena.new_pair(first, rest)
    }

    // create a new atom whose value is the given slice of the specified atom
    fn new_substr(
        &mut self,
        node: Self::Ptr,
        start: u32,
        end: u32,
    ) -> Result<Self::Ptr, EvalErr<Self::Ptr>> {
        self.arena.borrow_mut().arena.new_substr(node, start, end)
    }

    fn atom<'a>(&'a self, node: &'a Self::Ptr) -> &'a [u8] {
        self.arena.borrow().arena.atom(node)
    }

    fn buf<'a>(&'a self, node: &'a Self::AtomBuf) -> &'a [u8] {
        self.arena.borrow().arena.buf(node)
    }

    fn sexp(&self, node: &Self::Ptr) -> SExp<Self::Ptr, Self::AtomBuf> {
        self.arena.borrow().arena.sexp(node)
    }

    fn null(&self) -> Self::Ptr {
        self.arena.borrow().arena.null()
    }

    fn one(&self) -> Self::Ptr {
        self.arena.borrow().arena.one()
    }
}
*/

#[pyclass(subclass, unsendable)]
pub struct PyIntAllocator {
    arena: IntAllocator,
}

#[pyclass(subclass, unsendable)]
pub struct PyIntNode {
    arena: PyObject, // &PyCell<PyIntAllocator>
    ptr: <IntAllocator as Allocator>::Ptr,
}

impl PyIntNode {
    fn allocator<'p>(&'p self, py: Python<'p>) -> PyResult<PyRef<'p, PyIntAllocator>> {
        let allocator: &PyCell<PyIntAllocator> = self.arena.extract(py)?;
        Ok(allocator.try_borrow()?)
    }

    fn allocator_mut<'p>(&'p self, py: Python<'p>) -> PyResult<PyRefMut<'p, PyIntAllocator>> {
        let allocator: &PyCell<PyIntAllocator> = self.arena.extract(py)?;
        Ok(allocator.try_borrow_mut()?)
    }
}

#[pymethods]
impl PyIntNode {
    #[getter(pair)]
    pub fn pair(&self, py: Python) -> PyResult<Option<PyObject>> {
        let allocator = self.allocator(py)?;
        let allocator: &IntAllocator = &allocator.arena;
        match allocator.sexp(&self.ptr) {
            SExp::Pair(p1, p2) => {
                {
                    let v: &PyTuple = PyTuple::new(py, &[p1, p2]);
                    let v: PyObject = v.into();
                    Ok(Some(v))
                }
            }
            _ => Ok(None),
        }
    }

    /*
    pub fn _pair(&self) -> Option<(PyNode, PyNode)> {
        match ArcAllocator::new().sexp(&self.node) {
            SExp::Pair(p1, p2) => Some((p1.into(), p2.into())),
            _ => None,
        }
    }
    */

    #[getter(atom)]
    pub fn atom(&self, py: Python) -> PyResult<Option<PyObject>> {
        let allocator = self.allocator(py)?;
        let allocator: &IntAllocator = &allocator.arena;
        match allocator.sexp(&self.ptr) {
            SExp::Atom(atom) => {
                let s: &[u8] = allocator.buf(&atom);
                let s: &PyBytes = PyBytes::new(py, s);
                let s: PyObject = s.into();
                Ok(Some(s))
            }
            _ => Ok(None),
        }
    }
}

/*
impl Allocator for &PyIntAllocator {
    type Ptr = i32;
    type AtomBuf = IntAtomBuf;

    fn new_atom(&mut self, v: &[u8]) -> Result<Self::Ptr, EvalErr<Self::Ptr>> {
        self.arena.new_atom(v)
    }

    fn new_pair(
        &mut self,
        first: Self::Ptr,
        rest: Self::Ptr,
    ) -> Result<Self::Ptr, EvalErr<Self::Ptr>> {
        self.arena.new_pair(first, rest)
    }

    // create a new atom whose value is the given slice of the specified atom
    fn new_substr(
        &mut self,
        node: Self::Ptr,
        start: u32,
        end: u32,
    ) -> Result<Self::Ptr, EvalErr<Self::Ptr>> {
        self.arena.new_substr(node, start, end)
    }

    fn atom<'a>(&'a self, node: &'a Self::Ptr) -> &'a [u8] {
        self.arena.atom(node)
    }

    fn buf<'a>(&'a self, node: &'a Self::AtomBuf) -> &'a [u8] {
        self.arena.buf(node)
    }

    fn sexp(&self, node: &Self::Ptr) -> SExp<Self::Ptr, Self::AtomBuf> {
        self.arena.sexp(node)
    }

    fn null(&self) -> Self::Ptr {
        self.arena.null()
    }

    fn one(&self) -> Self::Ptr {
        self.arena.one()
    }
}
*/

impl PythonGateway<IntAllocator> for &PyCachingAllocator {
    fn to_pyobject(self, py: Python, ptr: <IntAllocator as Allocator>::Ptr) -> PyResult<PyObject> {
        let arena = self.arena.clone();
        let node = PyIntNode { arena, ptr };
        let cell = PyCell::new(py, node)?;
        Ok(cell.into_py(py))
    }

    fn from_pyobject(self, py: Python, o: PyObject) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        let obj: &PyCell<PyIntNode> = o.extract(py)?;
        Ok(obj.try_borrow()?.ptr)
    }
}
