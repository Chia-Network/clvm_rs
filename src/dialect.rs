use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::reduction::Response;

#[repr(u32)]
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum Operators {
    None,
    BLS,
}

pub trait Dialect {
    fn quote_kw(&self) -> &[u8];
    fn apply_kw(&self) -> &[u8];
    fn softfork_kw(&self) -> &[u8];
    fn softfork_extension(&self, ext: u32) -> Operators;
    fn op(
        &self,
        allocator: &mut Allocator,
        op: NodePtr,
        args: NodePtr,
        max_cost: Cost,
        extensions: Operators,
    ) -> Response;
    fn stack_limit(&self) -> usize;
    fn allow_unknown_ops(&self) -> bool;
}
