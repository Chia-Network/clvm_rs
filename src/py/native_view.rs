use pyo3::prelude::PyObject;

use crate::allocator::Allocator;
use crate::int_allocator::IntAllocator;

#[derive(Clone)]
pub struct NativeView {
    pub arena: PyObject, // PyCell<PyIntAllocator>,
    pub ptr: <IntAllocator as Allocator>::Ptr,
}

impl NativeView {
    pub fn new(arena: PyObject, ptr: <IntAllocator as Allocator>::Ptr) -> Self {
        NativeView { arena, ptr }
    }
}
