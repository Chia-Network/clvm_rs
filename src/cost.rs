use crate::allocator::Allocator;
use crate::error::{CLVMResult, EvalErr};

pub type Cost = u64;

pub fn check_cost(a: &Allocator, cost: Cost, max_cost: Cost) -> CLVMResult<()> {
    if cost > max_cost {
        Err(EvalErr::CostExceeded(a.nil()))
    } else {
        Ok(())
    }
}
