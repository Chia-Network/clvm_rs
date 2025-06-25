use crate::allocator::Allocator;
use crate::error::EvalErr;

pub type Cost = u64;

pub fn check_cost(a: &Allocator, cost: Cost, max_cost: Cost) -> Result<(), EvalErr> {
    if cost > max_cost {
        Err(EvalErr(a.nil(), "cost exceeded".into()))
    } else {
        Ok(())
    }
}
