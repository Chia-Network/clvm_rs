use std::borrow::Borrow;
use std::cell::{Cell, Ref, RefCell};

use pyo3::{exceptions::PyBufferError, prelude::*};
//use pyo3::prelude::{PyObject, PyResult, Python};
use pyo3::ffi::Py_None;
use pyo3::types::PyBytes;
use pyo3::types::PyTuple;
use pyo3::AsPyPointer;

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

struct PyView {
    atom: PyObject,
    pair: PyObject,
}

impl PyView {
    fn py_bytes<'p>(&'p self, py: Python<'p>) -> Option<&'p PyBytes> {
        // this glue returns a &[u8] if self.atom has PyBytes behind it
        let r: Option<&PyBytes> = self.atom.extract(py).ok();
        r
    }

    fn py_pair<'p>(
        &'p self,
        py: Python<'p>,
    ) -> Option<(&'p PyCell<PyIntNode>, &'p PyCell<PyIntNode>)> {
        let args: &PyTuple = self.pair.extract(py).ok()?;
        let p0: &'p PyCell<PyIntNode> = args.get_item(0).extract().unwrap();
        let p1: &'p PyCell<PyIntNode> = args.get_item(1).extract().unwrap();
        Some((p0, p1))
    }
}

#[pyclass(subclass, unsendable)]
pub struct PyIntNode {
    arena: PyObject, // &PyCell<PyIntAllocator>
    // rust view
    native_view: Cell<Option<<IntAllocator as Allocator>::Ptr>>,
    // python view
    py_view: RefCell<Option<PyView>>,
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

    fn ptr(&mut self, py: Option<Python>) -> <IntAllocator as Allocator>::Ptr {
        if let Some(r) = self.native_view.get() {
            r.clone()
        } else {
            if let Some(py) = py {
                self.ensure_native_view(py)
            } else {
                panic!("can't cast from python to native")
            }
        }
    }

    fn ensure_native_view(&mut self, py: Python) -> <IntAllocator as Allocator>::Ptr {
        let mut allocator = self.allocator_mut(py).unwrap();
        let mut allocator: &mut IntAllocator = &mut allocator.arena;
        let mut to_cast: Vec<&PyIntNode> = vec![self];
        loop {
            let t = to_cast.pop();
            match t {
                None => break,
                Some(t) => {
                    if t.native_view.get().is_none() {
                        let py_view = self.py_view.borrow();
                        let py_view = py_view.as_ref().unwrap();
                        match py_view.py_bytes(py) {
                            Some(blob) => {
                                let new_ptr = allocator.new_atom(blob.as_bytes()).unwrap();
                                t.native_view.set(Some(new_ptr));
                            }
                            None => {
                                let (p1, p2) = py_view.py_pair(py).unwrap();
                                // TODO: finish this
                            }
                        }
                    }
                }
            }
        }
        self.native_view.get().unwrap()
    }

    fn ensure_python_view<'p>(&'p mut self, py: Python<'p>) -> PyResult<Ref<'p, Option<PyView>>> {
        if self.py_view.borrow().is_none() {
            let mut allocator: &mut IntAllocator = &mut self.allocator_mut(py)?.arena;
            let mut to_cast: Vec<&PyIntNode> = vec![self];
            loop {
                let t = to_cast.pop();
                match t {
                    None => break,
                    Some(t) => {
                        if t.py_view.borrow().is_some() {
                            continue;
                        }
                        let ptr = t.native_view.get().unwrap();
                        match allocator.sexp(&ptr) {
                            SExp::Atom(a) => {
                                let as_u8: &[u8] = allocator.buf(&a);
                                let py_bytes = PyBytes::new(py, as_u8);
                                let py_object: PyObject = py_bytes.to_object(py);
                                let py_view = PyView {
                                    atom: py_object,
                                    pair: ().to_object(py),
                                };
                                t.py_view.replace(Some(py_view));
                            }
                            SExp::Pair(p1, p2) => {
                                let r1 = allocator.sexp(&p1);
                                let r2 = allocator.sexp(&p2);
                                // TODO: finish this
                            }
                        }
                    }
                }
            }
        }
        Ok(self.py_view.borrow())
    }
}

#[pymethods]
impl PyIntNode {
    #[getter(pair)]
    pub fn pair<'p>(&'p mut self, py: Python<'p>) -> PyResult<PyObject> {
        let t: Ref<'p, Option<PyView>> = self.ensure_python_view(py)?;
        let t1 = &t.as_ref().unwrap().pair;
        Ok(t1.clone())

        /*
        let allocator = self.allocator(py)?;
        let allocator: &IntAllocator = &allocator.arena;
        match allocator.sexp(&self.ptr) {
            SExp::Pair(p1, p2) => {
                let v: &PyTuple = PyTuple::new(py, &[p1, p2]);
                let v: PyObject = v.into();
                Ok(Some(v))
            }
            _ => Ok(None),
        }*/
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
    pub fn atom<'p>(&'p mut self, py: Python<'p>) -> PyResult<PyObject> {
        let t: Ref<'p, Option<PyView>> = self.ensure_python_view(py)?;
        let t1 = &t.as_ref().unwrap().atom;
        Ok(t1.clone())

        /*
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
        */
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

/*
impl PythonGateway<IntAllocator> for &PyCachingAllocator {
    fn to_pyobject(self, py: Python, ptr: <IntAllocator as Allocator>::Ptr) -> PyResult<PyObject> {
        let arena = self.arena.clone();
        let node = PyIntNode {
            arena,
            native_view: Some(ptr),
            py_view: None,
        };
        let cell = PyCell::new(py, node)?;
        Ok(cell.into_py(py))
    }

    fn from_pyobject(self, py: Python, o: PyObject) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        let obj: &PyCell<PyIntNode> = o.extract(py)?;
        Ok(obj.try_borrow()?.ptr)
    }
}
*/
