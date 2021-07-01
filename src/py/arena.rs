/// An `Arena` is a collection of objects representing a program and
/// its arguments, and intermediate values reached while running
/// a program. Objects can be created in an `Arena` but are never
/// dropped until the `Arena` is dropped.
use std::cell::{RefCell, RefMut};

use pyo3::prelude::pyclass;
use pyo3::prelude::*;

use super::bridge_cache::BridgeCache;
use crate::allocator::Allocator;
use crate::serialize::node_from_bytes;

#[pyclass(subclass, unsendable)]
pub struct Arena {
    arena: RefCell<Allocator>,
    pub cache: BridgeCache,
    // TODO: make this private, remove this `pub`
}

#[pymethods]
impl Arena {
    #[new]
    pub fn new(py: Python, new_obj_f: PyObject) -> PyResult<Self> {
        Ok(Arena {
            arena: RefCell::new(Allocator::default()),
            cache: BridgeCache::new(py, new_obj_f)?,
        })
    }

    /// deserialize `bytes` into an object in this `Arena`
    pub fn deserialize<'p>(&self, py: Python<'p>, blob: &[u8]) -> PyResult<&'p PyAny> {
        let allocator: &mut Allocator = &mut self.allocator() as &mut Allocator;
        let ptr = node_from_bytes(allocator, blob)?;
        self.cache.py_for_native(py, ptr, allocator)
    }

    /// copy this python object into this `Arena` if it's not yet in the cache
    /// (otherwise it returns the previously cached object)
    pub fn include<'p>(&self, py: Python<'p>, obj: &'p PyAny) -> PyResult<&'p PyAny> {
        let ptr = Self::ptr_for_obj(self, py, obj)?;
        self.cache
            .py_for_native(py, ptr, &mut self.allocator() as &mut Allocator)
    }

    /// copy this python object into this `Arena` if it's not yet in the cache
    /// (otherwise it returns the previously cached object)
    pub fn ptr_for_obj(&self, py: Python, obj: &PyAny) -> PyResult<i32> {
        let allocator: &mut Allocator = &mut self.allocator() as &mut Allocator;
        self.cache.populate_native(py, obj, allocator)
    }
}

impl Arena {
    pub fn new_cell_obj(py: Python, new_obj_f: PyObject) -> PyResult<&PyCell<Self>> {
        PyCell::new(py, Arena::new(py, new_obj_f)?)
    }

    pub fn new_cell(py: Python) -> PyResult<&PyCell<Self>> {
        Self::new_cell_obj(py, py.eval("lambda sexp: sexp", None, None)?.to_object(py))
    }

    pub fn obj_for_ptr<'p>(&self, py: Python<'p>, ptr: i32) -> PyResult<&'p PyAny> {
        self.cache.py_for_native(py, ptr, &mut self.allocator())
    }

    pub fn allocator(&self) -> RefMut<Allocator> {
        self.arena.borrow_mut()
    }
}
