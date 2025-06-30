use crate::allocator::Allocator;
use crate::error::{EvalErr, Result};

pub type Cost = u64;

pub fn check_cost(a: &Allocator, cost: Cost, max_cost: Cost) -> Result<()> {
    if cost > max_cost {
        Err(EvalErr::CostExceeded(a.nil()))
    } else {
        Ok(())
    }
}
