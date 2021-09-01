use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::reduction::Response;

pub trait Dialect {
    fn quote_kw(&self) -> &[u8];
    fn apply_kw(&self) -> &[u8];
    fn op(&self, allocator: &mut Allocator, op: NodePtr, args: NodePtr, max_cost: Cost)
        -> Response;
}
