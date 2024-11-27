use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::reduction::Response;

/// The set of operators that are available in the dialect.
#[repr(u32)]
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum OperatorSet {
    /// Any softfork extensions that are not added yet will be rejected.
    Default,

    /// Originally added BLS operators when inside softfork extension 0.
    /// The operators have since been hardforked into the main operator set.
    Bls,

    /// The keccak256 operator, which is only available inside the softfork guard.
    /// This uses softfork extension 1, which does not conflict with the BLS fork.
    Keccak,
}

pub trait Dialect {
    fn quote_kw(&self) -> u32;
    fn apply_kw(&self) -> u32;
    fn softfork_kw(&self) -> u32;
    fn softfork_extension(&self, ext: u32) -> OperatorSet;
    fn op(
        &self,
        allocator: &mut Allocator,
        op: NodePtr,
        args: NodePtr,
        max_cost: Cost,
        extensions: OperatorSet,
    ) -> Response;
    fn allow_unknown_ops(&self) -> bool;
}
