use crate::allocator::Allocator;
use crate::allocator::NodePtr;
use crate::cost::Cost;
use crate::reduction::Response;

pub trait OperatorHandler {
    fn op(&self, allocator: &mut Allocator, op: NodePtr, args: NodePtr, max_cost: Cost)
        -> Response;
}
