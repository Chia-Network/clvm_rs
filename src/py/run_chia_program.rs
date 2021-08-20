use crate::allocator::Allocator;
use crate::cost::Cost;
use crate::py::lazy_node::LazyNode;
use crate::serialize::node_from_bytes;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::rc::Rc;

#[pyfunction]
pub fn run_chia_program(
    py: Python,
    program: &[u8],
    args: &[u8],
    max_cost: Cost,
    flags: u32,
) -> PyResult<(Cost, LazyNode)> {
    let mut allocator = Allocator::new();
    let program = node_from_bytes(&mut allocator, program)?;
    let args = node_from_bytes(&mut allocator, args)?;

    let r = py.allow_threads(|| {
        crate::gen::run_chia_program::run_chia_program(
            &mut allocator,
            program,
            args,
            max_cost,
            flags,
        )
    });
    match r {
        Ok(reduction) => {
            let val = LazyNode::new(Rc::new(allocator), reduction.1);
            Ok((reduction.0, val))
        }
        Err(eval_err) => {
            let node = LazyNode::new(Rc::new(allocator), eval_err.0);
            let msg = eval_err.1;
            let ctx: &PyDict = PyDict::new(py);
            ctx.set_item("msg", msg)?;
            ctx.set_item("node", node)?;
            Err(py
                .run(
                    "
from clvm.EvalError import EvalError
raise EvalError(msg, node)",
                    None,
                    Some(ctx),
                )
                .unwrap_err())
        }
    }
}
