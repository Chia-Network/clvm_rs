use std::rc::Rc;

use crate::lazy_node::LazyNode;
use clvmr::allocator::Allocator;
use clvmr::reduction::Response;

use clvmr::error::EvalErr;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

pub fn adapt_response(
    py: Python,
    allocator: Allocator,
    response: Response,
) -> PyResult<(u64, LazyNode)> {
    match response {
        Ok(reduction) => {
            let val = LazyNode::new(Rc::new(allocator), reduction.1);
            Ok((reduction.0, val))
        }
        Err(eval_err) => {
            let sexp =
                LazyNode::new(Rc::new(allocator), EvalErr::node_ptr(&eval_err)).to_object(py);
            let msg = eval_err.to_string().to_object(py);
            let tuple = PyTuple::new_bound(py, [msg, sexp]);
            let value_error: PyErr = PyValueError::new_err(tuple.to_object(py));
            Err(value_error)
        }
    }
}
