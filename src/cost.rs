use crate::allocator::Allocator;
use crate::reduction::EvalErr;

pub type Cost = u64;

pub fn check_cost(a: &Allocator, cost: Cost, max_cost: Cost) -> Result<(), EvalErr> {
    if cost > max_cost {
        Err(EvalErr(a.null(), "cost exceeded".into()))
    } else {
        Ok(())
    }
}
