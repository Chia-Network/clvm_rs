use crate::allocator::Allocator;
use crate::reduction::EvalErr;

// this must be small enough to prevent overflow in intermediate values in cost
// computations.
pub const MAX_ATOM_SIZE: usize = 8 * 1024 * 1024;

pub type Cost = u64;

pub fn check_cost<A: Allocator>(a: &A, cost: Cost, max_cost: Cost) -> Result<(), EvalErr<A::Ptr>> {
    if cost > max_cost {
        Err(EvalErr(a.null(), "cost exceeded".into()))
    } else {
        Ok(())
    }
}
