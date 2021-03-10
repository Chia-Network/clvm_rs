use std::borrow::Borrow;
use std::cell::{Ref, RefCell};

use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyBytes, PyTuple, PyType};

use crate::allocator::{Allocator, SExp};
use crate::int_allocator::IntAllocator;

use super::native_view::NativeView;
use super::py_int_allocator::PyIntAllocator;
use super::py_view::PyView;

#[derive(Clone)]
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
            .eval("__import__('weakref').WeakKeyDictionary()", None, None)?
            .to_object(py);

        let node = PyNaNode {
            node: view.clone(),
            int_arena_cache,
        };

        let r = PyCell::new(py, node)?;
        if let View::Native(native_view) = view {
            r.borrow()
                .add_to_arena_cache(py, &native_view.arena, native_view.ptr)?;
        }
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
        Self::ensure_native_view_cached(slf.borrow_mut(), py, arena, allocator)?;
        Ok(slf.try_borrow()?.check_cache(py, arena)?.unwrap())
    }

    pub fn ensure_native_view_cached<'p>(
        slf: PyRefMut<Self>,
        py: Python<'p>,
        arena: &PyObject,
        allocator: &mut IntAllocator,
    ) -> PyResult<()> {
        let mut to_cast: Vec<PyRefMut<Self>> = vec![slf];
        Ok(loop {
            let t: Option<PyRefMut<Self>> = to_cast.pop();
            match t {
                None => break,
                Some(mut node_ref) => {
                    let transfer: Option<(&'p PyCell<Self>, &'p PyCell<Self>)> =
                        node_ref.add_to_native_cache(py, arena, allocator)?;
                    if let Some((p0, p1)) = transfer {
                        to_cast.push(p0.borrow_mut());
                        to_cast.push(p1.borrow_mut());
                    }
                }
            }
        })
    }

    /// This instance has a corresponding rep in some `IntAllocator`
    /// Notate this in the cache.
    fn add_to_native_cache<'p>(
        &mut self,
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
                        let pair: &'p PyAny = pair.into_ref(py);
                        let pair: &'p PyTuple = pair.extract()?;

                        let p0: &'p PyCell<PyNaNode> = pair.get_item(0).extract()?;
                        let p1: &'p PyCell<PyNaNode> = pair.get_item(1).extract()?;
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

    /// If this instance is using `NativeView`, replace it with an equivalent `PythonView`
    /// so it can be use from python.
    pub fn ensure_python_view<'p>(
        &mut self,
        py: Python<'p>,
        borrowed_arena: Option<(&PyObject, &mut IntAllocator)>,
    ) -> PyResult<PyView> {
        // if using `NativeView`, swap it out for `PythonView`
        match self.node.clone() {
            View::Python(py_view) => Ok(py_view.clone()),
            View::Native(native_view) => {
                let mut py_int_allocator: PyRefMut<PyIntAllocator> =
                    native_view.arena.extract(py)?;
                let mut allocator_to_use: &mut IntAllocator = &mut py_int_allocator.arena;

                if let Some((arena, allocator)) = borrowed_arena {
                    // WARNING: this check may not actually work
                    // TODO: research & fix
                    if arena == &native_view.arena {
                        allocator_to_use = allocator;
                    }
                };
                self.ensure_py_view_for_ptr_allocator(
                    py,
                    &native_view.arena,
                    allocator_to_use,
                    native_view.ptr,
                )
            }
        }
    }

    fn ensure_py_view_for_ptr_allocator(
        &mut self,
        py: Python,
        arena: &PyObject,
        allocator: &mut IntAllocator,
        ptr: <IntAllocator as Allocator>::Ptr,
    ) -> PyResult<PyView> {
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
        let r = py_view.clone();
        let view = View::Python(py_view);
        self.node = view;
        Ok(r)
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
            PyView::Atom(obj) => Ok(obj.into_ref(py)),
            _ => Ok(py.eval("None", None, None)?.extract()?),
        }
    }

    #[getter(pair)]
    pub fn pair<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<&'p PyAny> {
        let py_view = slf.try_borrow_mut()?.ensure_python_view(py, None)?;
        match py_view {
            PyView::Pair(obj) => Ok(obj.into_ref(py)),
            _ => Ok(py.eval("None", None, None)?.extract()?),
        }
    }

    #[getter(cache)]
    fn cache<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        self.int_arena_cache.extract(py)
    }
}
