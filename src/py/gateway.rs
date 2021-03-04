use pyo3::prelude::{PyObject, PyResult, Python};

use crate::allocator::Allocator;

pub trait PythonGateway<A: Allocator> {
    fn to_pyobject(self, py: Python, ptr: <A as Allocator>::Ptr) -> PyResult<PyObject>;
    fn from_pyobject(self, py: Python, o: PyObject) -> PyResult<<A as Allocator>::Ptr>;
}
