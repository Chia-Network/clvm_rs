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

    fn ptr(slf: &PyCell<Self>, py: Option<Python>) -> <IntAllocator as Allocator>::Ptr {
        if let Some(r) = slf.borrow().native_view.get() {
            r
        } else {
            if let Some(py) = py {
                let p = slf.borrow();
                let mut allocator = p.allocator_mut(py).unwrap();
                let mut allocator: &mut IntAllocator = &mut allocator.arena;

                let mut to_cast: Vec<PyObject> = vec![slf.to_object(py)];

                Self::ensure_native_view(to_cast, allocator, py);
                slf.borrow().native_view.get().unwrap()
            } else {
                panic!("can't cast from python to native")
            }
        }
    }

    fn ensure_native_view<'p>(
        mut to_cast: Vec<PyObject>,
        allocator: &mut IntAllocator,
        py: Python<'p>,
    ) -> () {
        loop {
            let t: Option<PyObject> = to_cast.pop();
            match t {
                None => break,
                Some(t0) => {
                    let t0_5: &PyAny = t0.extract(py).unwrap();
                    let t1: &PyCell<Self> = t0_5.downcast().unwrap();
                    let t2: PyRef<Self> = t1.borrow();
                    if t2.native_view.get().is_none() {
                        let py_view_ref: Ref<Option<PyView>> = t2.py_view.borrow();
                        let py_view = py_view_ref.as_ref().unwrap();
                        match py_view.py_bytes(py) {
                            Some(blob) => {
                                let new_ptr = allocator.new_atom(blob.as_bytes()).unwrap();
                                t2.native_view.set(Some(new_ptr));
                            }
                            None => {
                                let (p1, p2) = py_view.py_pair(py).unwrap();
                                // check if both p1 and p2 have native views
                                // if so build and cache native view for t
                                let r1: Option<<IntAllocator as Allocator>::Ptr> =
                                    p1.borrow().native_view.get();
                                let r2: Option<<IntAllocator as Allocator>::Ptr> =
                                    p2.borrow().native_view.get();
                                if r1.is_some() && r2.is_some() {
                                    let s1 = r1.unwrap();
                                    let s2 = r2.unwrap();
                                    let ptr = allocator.new_pair(s1, s2).unwrap();
                                    t2.native_view.set(Some(ptr));
                                } else {
                                    // otherwise, push t, push p1, push p2 back on stack to be processed
                                    //
                                    // UGH, these objects are type `PyCell<PyIntNode>` not &PyIntNode, what do I do
                                    //
                                    to_cast.push(p1.to_object(py));
                                    to_cast.push(p2.to_object(py));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn ensure_python_view<'p>(
        mut to_cast: Vec<PyObject>,
        allocator: &mut IntAllocator,
        py: Python<'p>,
    ) -> PyResult<()> {
        loop {
            let t = to_cast.pop();
            match t {
                None => break,
                Some(t0) => {
                    let t1: &PyAny = t0.extract(py).unwrap();
                    let t2: &PyCell<Self> = t1.downcast().unwrap();
                    let t3: PyRef<Self> = t2.borrow();

                    if t3.py_view.borrow().is_some() {
                        continue;
                    }
                    let ptr = t3.native_view.get().unwrap();
                    match allocator.sexp(&ptr) {
                        SExp::Atom(a) => {
                            let as_u8: &[u8] = allocator.buf(&a);
                            let py_bytes = PyBytes::new(py, as_u8);
                            let py_object: PyObject = py_bytes.to_object(py);
                            let py_view = PyView {
                                atom: py_object,
                                pair: ().to_object(py),
                            };
                            t3.py_view.replace(Some(py_view));
                        }
                        SExp::Pair(p1, p2) => {
                            // create new n1, n2 child nodes of t
                            let arena = t3.arena.clone();
                            let native_view = Cell::new(Some(p1));
                            let py_view = RefCell::new(None);
                            let n1 = PyCell::new(
                                py,
                                PyIntNode {
                                    arena,
                                    native_view,
                                    py_view,
                                },
                            )?;
                            let arena = t3.arena.clone();
                            let native_view = Cell::new(Some(p2));
                            let py_view = RefCell::new(None);
                            let n2 = PyCell::new(
                                py,
                                PyIntNode {
                                    arena,
                                    native_view,
                                    py_view,
                                },
                            )?;
                            let py_object = PyTuple::new(py, &[n1, n2]);
                            let py_view = PyView {
                                pair: py_object.to_object(py),
                                atom: ().to_object(py),
                            };
                            t3.py_view.replace(Some(py_view));
                            to_cast.push(n1.to_object(py));
                            to_cast.push(n2.to_object(py));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[pymethods]
impl PyIntNode {
    #[getter(pair)]
    pub fn pair<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<PyObject> {
        let t0: PyRef<PyIntNode> = slf.borrow();
        let mut t1: PyRefMut<PyIntAllocator> = t0.allocator_mut(py)?;
        let allocator: &mut IntAllocator = &mut t1.arena;
        Self::ensure_python_view(vec![slf.to_object(py)], allocator, py)?;
        let t2: Ref<Option<PyView>> = t0.py_view.borrow();
        let t3 = &t2.as_ref().unwrap().pair;
        Ok(t3.clone())

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
    pub fn atom<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<PyObject> {
        let t0: PyRef<PyIntNode> = slf.borrow();
        let mut t1: PyRefMut<PyIntAllocator> = t0.allocator_mut(py)?;
        let allocator: &mut IntAllocator = &mut t1.arena;
        Self::ensure_python_view(vec![slf.to_object(py)], allocator, py)?;
        let t2: Ref<Option<PyView>> = t0.py_view.borrow();
        let t3 = &t2.as_ref().unwrap().atom;
        Ok(t3.clone())
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
