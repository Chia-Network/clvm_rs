use crate::allocator::Allocator;
use crate::node::Node;
use crate::reduction::Response;

pub type OpFn<T> = fn(&Node<T>) -> Response<<T as Allocator>::Ptr>;
