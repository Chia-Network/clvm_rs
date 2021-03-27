use std::cell::RefMut;

use pyo3::prelude::{pyclass, pymethods};
use pyo3::PyCell;
use pyo3::PyObject;
use pyo3::PyRef;
use pyo3::PyResult;
use pyo3::Python;
use pyo3::ToPyObject;

use crate::int_allocator::IntAllocator;

use super::clvm_object::CLVMObject;
use super::py_arena::PyArena;

#[pyclass(weakref, subclass)]
pub struct NativeCLVMObject {
    arena: PyObject,
    ptr: i32,
}

impl NativeCLVMObject {
    pub fn new(py: Python, arena: &PyCell<PyArena>, ptr: i32) -> Self {
        let arena = arena.to_object(py);
        Self { arena, ptr }
    }
}

#[pymethods]
impl NativeCLVMObject {
    fn clvm_object<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyCell<CLVMObject>> {
        let arena: PyRef<PyArena> = self.arena.extract(py)?;
        let mut allocator: RefMut<IntAllocator> = arena.allocator();
        arena.py_for_native(py, &self.ptr, &mut allocator)
    }

    #[getter(arena)]
    fn arena<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyCell<PyArena>> {
        self.arena.extract(py)
    }
}
