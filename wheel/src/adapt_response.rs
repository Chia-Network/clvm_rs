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
            let sexp: Bound<'_, PyAny> = Bound::new(
                py,
                LazyNode::new(Rc::new(allocator), EvalErr::node_ptr(&eval_err)),
            )?
            .into_any();
            let msg: Bound<'_, PyAny> = eval_err.to_string().into_pyobject(py)?.into_any();
            let tuple = PyTuple::new(py, [msg, sexp])?;
            let value_error: PyErr = PyValueError::new_err(tuple.unbind().into_any());
            Err(value_error)
        }
    }
}
