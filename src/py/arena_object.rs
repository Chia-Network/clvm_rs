use std::cell::RefMut;

use pyo3::prelude::{pyclass, pymethods};
use pyo3::PyAny;
use pyo3::PyCell;
use pyo3::PyObject;
use pyo3::PyResult;
use pyo3::Python;
use pyo3::ToPyObject;

use crate::int_allocator::IntAllocator;

use super::py_arena::PyArena;

#[pyclass(weakref, subclass)]
pub struct ArenaObject {
    arena: PyObject,
    ptr: i32,
}

impl ArenaObject {
    pub fn new(py: Python, arena: &PyCell<PyArena>, ptr: i32) -> Self {
        let arena = arena.to_object(py);
        Self { arena, ptr }
    }
}

#[pymethods]
impl ArenaObject {
    fn clvm_object<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        let arena: &PyCell<PyArena> = self.arena.extract(py)?;
        let arena_ptr = arena.borrow();
        let mut allocator: RefMut<IntAllocator> = arena_ptr.allocator();
        arena_ptr.py_for_native(py, &self.ptr, &mut allocator)
    }

    #[getter(arena)]
    pub fn get_arena<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyCell<PyArena>> {
        self.arena.extract(py)
    }

    #[getter(ptr)]
    pub fn get_ptr(&self) -> i32 {
        self.ptr
    }

    #[getter(atom)]
    pub fn get_atom<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        self.clvm_object(py)?.getattr("atom")
    }

    #[getter(pair)]
    pub fn get_pair<'p>(&'p self, py: Python<'p>) -> PyResult<&'p PyAny> {
        self.clvm_object(py)?.getattr("pair")
    }
}

impl From<&ArenaObject> for i32 {
    fn from(obj: &ArenaObject) -> Self {
        obj.ptr
    }
}
