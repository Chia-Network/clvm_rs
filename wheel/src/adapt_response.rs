use std::rc::Rc;

use crate::lazy_node::LazyNode;
use clvmr::allocator::Allocator;
use clvmr::reduction::Response;

use pyo3::prelude::*;

pub fn adapt_response(
    py: Python,
    allocator: Allocator,
    response: Response,
) -> PyResult<(PyObject, LazyNode)> {
    match response {
        Ok(reduction) => {
            let val = LazyNode::new(Rc::new(allocator), reduction.1);
            let rv: PyObject = reduction.0.into_py(py);
            Ok((rv, val))
        }
        Err(eval_err) => {
            let rv: PyObject = eval_err.1.into_py(py);
            let val = LazyNode::new(Rc::new(allocator), eval_err.0);
            Ok((rv, val))
        }
    }
}
