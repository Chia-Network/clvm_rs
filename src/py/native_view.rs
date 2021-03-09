use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

use crate::allocator::Allocator;
use crate::int_allocator::IntAllocator;

pub struct NativeView {
    pub arena: PyObject, // PyCell<PyIntAllocator>,
    pub ptr: <IntAllocator as Allocator>::Ptr,
}

impl NativeView {
    pub fn new(arena: PyObject, ptr: <IntAllocator as Allocator>::Ptr) -> Self {
        NativeView { arena, ptr }
    }
}
