use std::cell::Cell;

use pyo3::prelude::*;
use pyo3::types::{IntoPyDict, PyBytes, PyTuple, PyType};

use crate::allocator::{Allocator, SExp};
use crate::int_allocator::IntAllocator;

use super::py_int_allocator::PyIntAllocator;
use super::py_view::PyView;

#[pyclass(weakref, subclass)]
pub struct PyNaNode {
    pub py_view: Option<PyView>,
    pub int_cache: Cell<Option<i32>>,
    //int_arena_cache: PyObject, // WeakKeyDict[PyIntAllocator, int]
}

pub fn new_cache(py: Python) -> PyResult<PyObject> {
    Ok(py
        .eval("__import__('weakref').WeakValueDictionary()", None, None)?
        .to_object(py))
}

pub fn add_to_cache(
    py: Python,
    cache: &PyObject,
    ptr: <IntAllocator as Allocator>::Ptr,
    value: &PyCell<PyNaNode>,
) -> PyResult<()> {
    //return Ok(());
    let locals = [
        ("cache", cache.clone()),
        ("key", ptr.to_object(py)),
        ("value", value.to_object(py)),
    ]
    .into_py_dict(py);

    Ok(py.run("cache[key] = value", None, Some(locals))?)
}

pub fn from_cache(
    py: Python,
    cache: &PyObject,
    ptr: &<IntAllocator as Allocator>::Ptr,
) -> PyResult<Option<PyObject>> {
    let locals = [("cache", cache.clone()), ("key", ptr.to_object(py))].into_py_dict(py);
    py.eval("cache.get(key)", None, Some(locals))?.extract()
}

pub fn apply_to_tree<T, F>(mut node: T, mut apply: F) -> PyResult<()>
where
    F: FnMut(T) -> PyResult<Option<(T, T)>>,
    T: Clone,
{
    let mut items = vec![node];
    loop {
        let t = items.pop();
        if let Some(obj) = t {
            if let Some((p0, p1)) = apply(obj.clone())? {
                items.push(obj);
                items.push(p0);
                items.push(p1);
            }
        } else {
            break;
        }
    }
    Ok(())
}

impl PyNaNode {
    pub fn new(py: Python, py_view: Option<PyView>) -> PyResult<&PyCell<Self>> {
        let int_cache = Cell::new(None);
        PyCell::new(py, PyNaNode { py_view, int_cache })
    }

    pub fn clear_native_view(slf: &PyCell<Self>, py: Python) -> PyResult<()> {
        apply_to_tree(slf.to_object(py), move |obj: PyObject| {
            let mut node: PyRefMut<Self> = obj.extract(py)?;
            assert!(node.py_view.is_some());
            Ok(if let Some(PyView::Pair(tuple)) = &node.py_view {
                let (p0, p1): (PyObject, PyObject) = tuple.extract(py)?;
                if node.int_cache.get().is_some() {
                    node.int_cache.set(None);
                    Some((p0, p1))
                } else {
                    None
                }
            } else {
                node.int_cache.set(None);
                None
            })
        })
    }
}

#[pymethods]
impl PyNaNode {
    #[new]
    fn new_obj(py: Python, obj: &PyAny) -> PyResult<Self> {
        Ok(if let Ok(tuple) = obj.extract() {
            let py_view = PyView::new_pair(py, tuple)?;
            Self {
                py_view: Some(py_view),
                int_cache: Cell::new(None),
            }
        } else {
            let py_bytes: &PyBytes = obj.extract()?;
            let py_view = PyView::new_atom(py, py_bytes);
            Self {
                py_view: Some(py_view),
                int_cache: Cell::new(None),
            }
        })
    }

    #[classmethod]
    fn new_atom<'p>(_cls: &PyType, py: Python<'p>, atom: &PyBytes) -> PyResult<&'p PyCell<Self>> {
        let py_view = PyView::new_atom(py, atom);
        Self::new(py, Some(py_view))
    }

    #[classmethod]
    fn new_pair<'p>(
        _cls: &PyType,
        py: Python<'p>,
        p1: &PyCell<PyNaNode>,
        p2: &PyCell<PyNaNode>,
    ) -> PyResult<&'p PyCell<Self>> {
        let tuple = PyTuple::new(py, &[p1, p2]);
        let py_view = PyView::new_pair(py, tuple)?;
        Self::new(py, Some(py_view))
    }

    #[classmethod]
    fn new_tuple<'p>(_cls: &PyType, py: Python<'p>, tuple: &PyTuple) -> PyResult<&'p PyCell<Self>> {
        let py_view = PyView::new_pair(py, tuple)?;
        Self::new(py, Some(py_view))
    }

    #[getter(atom)]
    pub fn atom<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<PyObject> {
        match slf.try_borrow()?.py_view.as_ref() {
            Some(PyView::Atom(obj)) => Ok(obj.clone()),
            _ => Ok(py.None()),
        }
    }

    #[getter(pair)]
    pub fn pair<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<PyObject> {
        match slf.try_borrow()?.py_view.as_ref() {
            Some(PyView::Pair(obj)) => Ok(obj.clone()),
            _ => Ok(py.None()),
        }
    }

    #[getter(native)]
    pub fn native<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<PyObject> {
        Ok(slf.borrow().int_cache.get().to_object(py))
    }

    #[getter(python)]
    pub fn python<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<PyObject> {
        Ok(match &slf.borrow().py_view {
            Some(PyView::Atom(atom)) => ("Atom", atom).to_object(py),
            Some(PyView::Pair(pair)) => ("Pair", pair).to_object(py),
            _ => py.None(),
        })
    }
}
