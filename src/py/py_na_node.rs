use std::borrow::Borrow;
use std::cell::{Ref, RefCell};

use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyBytes, PyTuple, PyType};

use crate::allocator::{Allocator, SExp};
use crate::int_allocator::IntAllocator;

use super::native_view::NativeView;
use super::py_int_allocator::PyIntAllocator;
use super::py_view::PyView;

enum View {
    Python(PyView),
    Native(NativeView),
}

#[pyclass]
pub struct PyNaNode {
    node: View,
    int_arena_cache: PyObject, // WeakKeyDict[PyIntAllocator, int]
}

impl PyNaNode {
    fn new(py: Python, view: View) -> PyResult<&PyCell<Self>> {
        let int_arena_cache = py
            .eval("import weakref; weakref.WeakKeyDictionary()", None, None)?
            .to_object(py);

        let node = PyNaNode {
            node: view,
            int_arena_cache,
        };
        if let View::Native(native_view) = view {
            node.add_to_arena_cache(py, &native_view.arena, native_view.ptr)?;
        }

        let r = PyCell::new(py, node)?;
        Ok(r)
    }

    fn add_to_arena_cache(
        &self,
        py: Python,
        arena: &PyObject,
        ptr: <IntAllocator as Allocator>::Ptr,
    ) -> PyResult<()> {
        let locals = [
            ("cache", self.int_arena_cache.clone()),
            ("key", arena.clone()),
            ("value", ptr.to_object(py)),
        ]
        .into_py_dict(py);

        py.run("cache[key] = value", None, Some(locals))?;
        Ok(())
    }

    pub fn from_ptr<'p>(
        py: Python<'p>,
        py_int_allocator: &PyObject,
        ptr: <IntAllocator as Allocator>::Ptr,
    ) -> PyResult<&'p PyCell<Self>> {
        let native_view = NativeView::new(py_int_allocator.clone(), ptr);
        Self::new(py, View::Native(native_view))
    }

    fn check_cache(
        &self,
        py: Python,
        key: &PyObject,
    ) -> PyResult<Option<<IntAllocator as Allocator>::Ptr>> {
        let locals = [
            ("cache", self.int_arena_cache.clone()),
            ("key", key.clone()),
        ]
        .into_py_dict(py);
        py.eval("cache.get(key)", None, Some(locals))?.extract()
    }

    pub fn ptr(
        slf: &PyCell<Self>,
        py: Python,
        arena: &PyObject,
        allocator: &mut IntAllocator,
    ) -> PyResult<<IntAllocator as Allocator>::Ptr> {
        {
            let self_ref = slf.try_borrow()?;
            match self_ref.check_cache(py, arena)? {
                Some(v) => return Ok(v),
                _ => (),
            }
        }
        Self::ensure_native_view_cached(slf, py, arena, allocator)?;
        Ok(slf.try_borrow()?.check_cache(py, arena)?.unwrap())
    }

    pub fn ensure_native_view_cached(
        slf: &PyCell<Self>,
        py: Python,
        arena: &PyObject,
        allocator: &mut IntAllocator,
    ) -> PyResult<()> {
        let mut to_cast: Vec<&PyCell<Self>> = vec![slf];
        Ok(loop {
            let t: Option<&PyCell<Self>> = to_cast.pop();
            match t {
                None => break,
                Some(py_cell) => {
                    let transfer: Option<(&PyCell<Self>, &PyCell<Self>)> = py_cell
                        .try_borrow_mut()?
                        .add_to_native_cache(py, arena, allocator)?;
                    if let Some((p0, p1)) = transfer {
                        to_cast.push(p0);
                        to_cast.push(p1);
                    }
                }
            }
        })
    }

    fn add_to_native_cache<'p>(
        &'p mut self,
        py: Python<'p>,
        arena: &PyObject,
        allocator: &mut IntAllocator,
    ) -> PyResult<Option<(&'p PyCell<Self>, &'p PyCell<Self>)>> {
        // if it's an atom, we add it to the allocator & cache the addition
        // if it's a pair, and BOTH are in the cache, we add to allocator & cache
        //  otherwise, we return both children so they can be cached (if necessary)
        if self.check_cache(py, arena)?.is_none() {
            let py_view = self.ensure_python_view(py, Some((arena, allocator)))?;
            let new_ptr = {
                match py_view {
                    PyView::Atom(obj) => {
                        let blob: &[u8] = obj.extract(py).unwrap();
                        let ptr = allocator.new_atom(blob).unwrap();
                        ptr
                    }
                    PyView::Pair(pair) => {
                        let pair: &PyTuple = pair.extract(py)?;

                        let p0: &PyCell<PyNaNode> = pair.get_item(0).extract()?;
                        let p1: &PyCell<PyNaNode> = pair.get_item(1).extract()?;
                        let ptr_0 = p0.borrow().check_cache(py, arena)?;
                        let ptr_1 = p1.borrow().check_cache(py, arena)?;
                        if let (Some(ptr_0), Some(ptr_1)) = (ptr_0, ptr_1) {
                            let ptr = allocator.new_pair(ptr_0, ptr_1).unwrap();
                            ptr
                        } else {
                            return Ok(Some((p0, p1)));
                        }
                    }
                }
            };
            self.add_to_arena_cache(py, &self.int_arena_cache, new_ptr)?;
        }
        Ok(None)
    }

    pub fn ensure_python_view<'p>(
        &'p mut self,
        py: Python<'p>,
        borrowed_arena: Option<(&PyObject, &mut IntAllocator)>,
    ) -> PyResult<&'p PyView> {
        // if using `NativeView`, swap it out for `PythonView`
        match &self.node {
            View::Python(py_view) => Ok(py_view),
            View::Native(native_view) => {
                let (arena, allocator) = {
                    if {
                        if let Some((arena, allocator)) = borrowed_arena {
                            // WARNING: this check may not actually work
                            // TODO: research & fix
                            arena == &native_view.arena
                        } else {
                            false
                        }
                    } {
                        borrowed_arena.unwrap()
                    } else {
                        let py_int_allocator: PyRef<PyIntAllocator> =
                            native_view.arena.extract(py)?;
                        let allocator = &mut py_int_allocator.arena;
                        (&native_view.arena, allocator)
                    }
                };
                self.ensure_py_view_for_ptr_allocator(py, arena, allocator, native_view.ptr)
            }
        }
    }

    fn ensure_py_view_for_ptr_allocator(
        &mut self,
        py: Python,
        arena: &PyObject,
        allocator: &mut IntAllocator,
        ptr: <IntAllocator as Allocator>::Ptr,
    ) -> PyResult<&PyView> {
        // create a PyView and put it in self
        let py_view = match allocator.sexp(&ptr) {
            SExp::Atom(a) => {
                let blob = allocator.buf(&a);
                let py_bytes = PyBytes::new(py, blob);
                PyView::new_atom(py, py_bytes)
            }
            SExp::Pair(ptr_1, ptr_2) => {
                let p1 = Self::from_ptr(py, &arena, ptr_1)?;
                let p2 = Self::from_ptr(py, &arena, ptr_2)?;
                PyView::new_pair(py, PyTuple::new(py, &[p1, p2]))?
            }
        };
        self.add_to_arena_cache(py, arena, ptr)?;
        let view = View::Python(py_view);
        self.node = view;
        Ok(&py_view)
    }
}

#[pymethods]
impl PyNaNode {
    #[classmethod]
    fn new_atom<'p>(cls: &PyType, py: Python<'p>, atom: &PyBytes) -> PyResult<&'p PyCell<Self>> {
        let node = PyView::new_atom(py, atom);
        Self::new(py, View::Python(node))
    }

    #[classmethod]
    fn new_pair<'p>(
        cls: &PyType,
        py: Python<'p>,
        p1: &PyCell<PyNaNode>,
        p2: &PyCell<PyNaNode>,
    ) -> PyResult<&'p PyCell<Self>> {
        let tuple = PyTuple::new(py, &[p1, p2]);

        let node = PyView::new_pair(py, tuple)?;
        Self::new(py, View::Python(node))
    }

    #[getter(atom)]
    pub fn atom<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<&'p PyAny> {
        let py_view = slf.try_borrow_mut()?.ensure_python_view(py, None)?;
        match py_view {
            PyView::Atom(obj) => Ok(obj.as_ref(py)),
            _ => Ok(py.eval("None", None, None)?.extract()?),
        }
    }

    #[getter(pair)]
    pub fn pair<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<&'p PyAny> {
        let py_view = slf.try_borrow_mut()?.ensure_python_view(py, None)?;
        match py_view {
            PyView::Pair(obj) => Ok(obj.as_ref(py)),
            _ => Ok(py.eval("None", None, None)?.extract()?),
        }
    }
}
