use std::cell::{Cell, Ref, RefCell};

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3::types::PyTuple;
use pyo3::types::PyType;

use crate::allocator::{Allocator, SExp};
use crate::int_allocator::IntAllocator;

#[pyclass(subclass, unsendable)]
pub struct PyIntAllocator {
    pub arena: IntAllocator,
}

pub struct PyView {
    atom: PyObject,
    pair: PyObject,
}

impl PyView {
    pub fn new(atom: &PyObject, pair: &PyObject) -> Self {
        let atom = atom.clone();
        let pair = pair.clone();
        PyView { atom, pair }
    }

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
    pub arena: PyObject, // &PyCell<PyIntAllocator>
    // rust view
    pub native_view: Cell<Option<<IntAllocator as Allocator>::Ptr>>,
    // python view
    pub py_view: RefCell<Option<PyView>>,
}

impl PyIntNode {
    pub fn new(arena: PyObject, native_view: Option<i32>, py_view: Option<PyView>) -> Self {
        let native_view = Cell::new(native_view);
        let py_view = RefCell::new(py_view);
        Self {
            arena,
            native_view,
            py_view,
        }
    }

    pub fn from_ptr<'p>(
        py: Python<'p>,
        py_int_allocator: PyObject,
        ptr: <IntAllocator as Allocator>::Ptr,
    ) -> PyResult<&'p PyCell<Self>> {
        let py_int_node = PyCell::new(py, PyIntNode::new(py_int_allocator, Some(ptr), None));
        py_int_node
    }

    fn allocator<'p>(&'p self, py: Python<'p>) -> PyResult<PyRef<'p, PyIntAllocator>> {
        let allocator: &PyCell<PyIntAllocator> = self.arena.extract(py)?;
        Ok(allocator.try_borrow()?)
    }

    fn allocator_mut<'p>(&'p self, py: Python<'p>) -> PyResult<PyRefMut<'p, PyIntAllocator>> {
        let allocator: &PyCell<PyIntAllocator> = self.arena.extract(py)?;
        Ok(allocator.try_borrow_mut()?)
    }

    pub fn ptr(
        slf: &PyCell<Self>,
        py: Option<Python>,
        arena: PyObject,
        allocator: &mut IntAllocator,
    ) -> <IntAllocator as Allocator>::Ptr {
        if let Some(r) = slf.borrow().native_view.get() {
            r
        } else {
            if let Some(py) = py {
                let p = slf.borrow();
                let mut to_cast: Vec<PyObject> = vec![slf.to_object(py)];

                Self::ensure_native_view(to_cast, arena, allocator, py);
                slf.borrow().native_view.get().unwrap()
            } else {
                panic!("can't cast from python to native")
            }
        }
    }

    /*
    pub fn get_py_view<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<Ref<'p, Option<PyView>>> {
        let t0: PyRef<PyIntNode> = slf.borrow();
        {
            let t1: Ref<Option<PyView>> = t0.py_view.borrow();
            if t1.is_none() {
                let mut t2: PyRefMut<PyIntAllocator> = t0.allocator_mut(py)?;
                let allocator: &mut IntAllocator = &mut t2.arena;
                Self::ensure_python_view(vec![slf.to_object(py)], allocator, py)?;
            }
        }
        Ok(t0.py_view.borrow())
    }
    */

    pub fn ensure_native_view(
        mut to_cast: Vec<PyObject>,
        arena: PyObject,
        allocator: &mut IntAllocator,
        py: Python,
    ) {
        loop {
            let t: Option<PyObject> = to_cast.pop();
            match t {
                None => break,
                Some(t0) => {
                    let t0_5: &PyAny = t0.extract(py).unwrap();
                    let t1: &PyCell<Self> = t0_5.downcast().unwrap();
                    let mut t2: PyRefMut<Self> = t1.borrow_mut();
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
                                if let (Some(s1), Some(s2)) = (r1, r2) {
                                    let ptr = allocator.new_pair(s1, s2).unwrap();
                                    t2.native_view.set(Some(ptr));
                                    t2.arena = arena.clone();
                                } else {
                                    // otherwise, push t, push p1, push p2 back on stack to be processed
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

    pub fn ensure_python_view(
        mut to_cast: Vec<PyObject>,
        allocator: &mut IntAllocator,
        py: Python,
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
    #[classmethod]
    fn new_atom(cls: &PyType, py: Python, atom: &PyBytes) -> PyResult<Self> {
        let none: PyObject = py.None();
        let py_view = Some(PyView::new(&atom.to_object(py), &none));
        Ok(PyIntNode::new(none, None, py_view))
    }

    #[classmethod]
    fn new_pair(
        cls: &PyType,
        py: Python,
        p1: &PyCell<PyIntNode>,
        p2: &PyCell<PyIntNode>,
    ) -> PyResult<Self> {
        let pair: &PyTuple = PyTuple::new(py, &[p1, p2]);
        let none: PyObject = py.None();
        // TODO: ensure `pair` is a tuple of two `PyIntNode`
        let py_view = Some(PyView::new(&none, &pair.to_object(py)));
        Ok(PyIntNode::new(none, None, py_view))
    }

    #[getter(arena)]
    pub fn get_arena(&self) -> PyObject {
        self.arena.clone()
    }

    #[getter(pair)]
    pub fn pair<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<PyObject> {
        let t0: PyRef<PyIntNode> = slf.borrow();
        let t1: Ref<Option<PyView>> = t0.py_view.borrow();
        if t1.is_none() {
            let mut t2: PyRefMut<PyIntAllocator> = t0.allocator_mut(py)?;
            let allocator: &mut IntAllocator = &mut t2.arena;
            Self::ensure_python_view(vec![slf.to_object(py)], allocator, py)?;
        }
        let t3 = &t1.as_ref().unwrap().pair;
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
        let t1: Ref<Option<PyView>> = t0.py_view.borrow();
        if t1.is_none() {
            let mut t2: PyRefMut<PyIntAllocator> = t0.allocator_mut(py)?;
            let allocator: &mut IntAllocator = &mut t2.arena;
            Self::ensure_python_view(vec![slf.to_object(py)], allocator, py)?;
        }
        let t3 = &t1.as_ref().unwrap().atom;
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
