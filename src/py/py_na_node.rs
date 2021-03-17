use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple, PyType};

use super::py_view::PyView;

#[pyclass(weakref, subclass)]
pub struct PyNode {
    pub py_view: Option<PyView>,
}

impl PyNode {
    pub fn new(py: Python, py_view: Option<PyView>) -> PyResult<&PyCell<Self>> {
        PyCell::new(py, PyNode { py_view })
    }
}

#[pymethods]
impl PyNode {
    #[new]
    fn new_obj(py: Python, obj: &PyAny) -> PyResult<Self> {
        Ok(if let Ok(tuple) = obj.extract() {
            let py_view = PyView::new_pair(py, tuple)?;
            Self {
                py_view: Some(py_view),
            }
        } else {
            let py_bytes: &PyBytes = obj.extract()?;
            let py_view = PyView::new_atom(py, py_bytes);
            Self {
                py_view: Some(py_view),
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
        p1: &PyCell<PyNode>,
        p2: &PyCell<PyNode>,
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

    #[getter(python)]
    pub fn python<'p>(slf: &'p PyCell<Self>, py: Python<'p>) -> PyResult<PyObject> {
        Ok(match &slf.borrow().py_view {
            Some(PyView::Atom(atom)) => ("Atom", atom).to_object(py),
            Some(PyView::Pair(pair)) => ("Pair", pair).to_object(py),
            _ => py.None(),
        })
    }
}
