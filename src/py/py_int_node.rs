use std::cell::RefMut;

use pyo3::prelude::{pyclass, pymethods};
use pyo3::PyCell;
use pyo3::PyObject;
use pyo3::PyRef;
use pyo3::PyResult;
use pyo3::Python;
use pyo3::ToPyObject;

use crate::int_allocator::IntAllocator;

use super::py_int_allocator::PyIntAllocator;
use super::py_node::PyNode;

#[pyclass(weakref, subclass)]
pub struct PyIntNode {
    py_int_allocator: PyObject,
    ptr: i32,
}

impl PyIntNode {
    pub fn new(py: Python, py_int_allocator: &PyCell<PyIntAllocator>, ptr: i32) -> Self {
        let py_int_allocator = py_int_allocator.to_object(py);
        Self {
            py_int_allocator,
            ptr,
        }
    }
}

#[pymethods]
impl PyIntNode {
    fn clvm_object<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyCell<PyNode>> {
        let py_int_allocator: PyRef<PyIntAllocator> = self.py_int_allocator.extract(py)?;
        let mut allocator: RefMut<IntAllocator> = py_int_allocator.allocator();
        py_int_allocator.py_for_native(py, &self.ptr, &mut allocator)
    }

    #[getter(arena)]
    fn arena<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyCell<PyIntAllocator>> {
        self.py_int_allocator.extract(py)
    }
}
