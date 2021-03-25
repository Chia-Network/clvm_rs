use pyo3::prelude::pyclass;

use crate::int_allocator::IntAllocator;

use super::f_table::OpFn;

//type OpFn<T> = fn(&mut T, <T as Allocator>::Ptr, Cost) -> Response<<T as Allocator>::Ptr>;

#[pyclass]
pub struct NativeOp {
    pub op: OpFn<IntAllocator>,
}

impl NativeOp {
    pub fn new(op: OpFn<IntAllocator>) -> Self {
        Self { op }
    }
}
