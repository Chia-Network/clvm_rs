use pyo3::prelude::pyclass;

use crate::int_allocator::IntAllocator;

#[pyclass(subclass, unsendable)]
pub struct PyIntAllocator {
    pub arena: IntAllocator,
}

impl Default for PyIntAllocator {
    fn default() -> Self {
        PyIntAllocator {
            arena: IntAllocator::default(),
        }
    }
}
