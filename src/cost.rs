use crate::int_allocator::{IntAllocator, NodePtr};
use crate::reduction::EvalErr;

pub type Cost = u64;

pub fn check_cost(a: &IntAllocator, cost: Cost, max_cost: Cost) -> Result<(), EvalErr<NodePtr>> {
    if cost > max_cost {
        Err(EvalErr(a.null(), "cost exceeded".into()))
    } else {
        Ok(())
    }
}
