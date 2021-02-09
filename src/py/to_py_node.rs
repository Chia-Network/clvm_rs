use pyo3::PyClass;

use crate::allocator::Allocator;

pub trait ToPyNode<N: PyClass>: Allocator {
    fn to_pynode(&self, ptr: &Self::Ptr) -> N;
}
