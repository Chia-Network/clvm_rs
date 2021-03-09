use std::cell::{Cell, Ref, RefCell};

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3::types::PyTuple;
use pyo3::types::PyType;

use crate::allocator::{Allocator, SExp};
use crate::int_allocator::IntAllocator;

#[pyclass(subclass, unsendable)]
pub struct PyIntAllocator {
    pub arena: IntAllocator,
}
