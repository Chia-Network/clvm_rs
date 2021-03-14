

use pyo3::basic::CompareOp;
use pyo3::prelude::*;



use pyo3::PyObjectProtocol;


use crate::int_allocator::IntAllocator;

#[pyclass(subclass, unsendable)]
pub struct PyIntAllocator {
    pub arena: IntAllocator,
}

impl PyIntAllocator {
    fn id(&self) -> isize {
        let arena: *const IntAllocator = &self.arena as *const IntAllocator;
        arena as isize
    }
}

impl Default for PyIntAllocator {
    fn default() -> Self {
        PyIntAllocator {
            arena: IntAllocator::default(),
        }
    }
}

#[pyproto]
impl PyObjectProtocol for PyIntAllocator {
    fn __richcmp__(&self, other: PyRef<PyIntAllocator>, _op: CompareOp) -> i8 {
        let t1 = self.id();
        let t2 = other.id();
        if t1 < t2 {
            -1
        } else if t2 < t1 {
            1
        } else {
            0
        }
    }

    fn __hash__(&self) -> isize {
        self.id()
    }
}
