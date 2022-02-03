use crate::lazy_node::LazyNode;
use clvmr::allocator::Allocator;
use clvmr::reduction::{EvalErr, Response};

use std::rc::Rc;

use pyo3::prelude::*;
use pyo3::types::PyDict;

pub fn eval_err_to_pyresult<T>(py: Python, eval_err: EvalErr, allocator: Allocator) -> PyResult<T> {
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

pub fn adapt_response_to_py(
    py: Python,
    allocator: Allocator,
    r: Response,
) -> PyResult<(u64, LazyNode)> {
    match r {
        Ok(reduction) => {
            let val = LazyNode::new(Rc::new(allocator), reduction.1);
            Ok((reduction.0, val))
        }
        Err(eval_err) => eval_err_to_pyresult(py, eval_err, allocator),
    }
}
