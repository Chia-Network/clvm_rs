use std::collections::HashMap;

use pyo3::prelude::pyclass;
use pyo3::types::PyString;
use pyo3::types::PyTuple;
use pyo3::PyCell;
use pyo3::PyErr;
use pyo3::PyObject;
use pyo3::PyResult;
use pyo3::Python;
use pyo3::ToPyObject;

use crate::allocator::Allocator;
use crate::cost::Cost;
use crate::int_allocator::IntAllocator;
use crate::py::f_table::FLookup;
use crate::py::int_allocator_gateway::PyIntNode;
use crate::reduction::{EvalErr, Reduction, Response};
use crate::run_program::OperatorHandler;

use super::py_node;

#[pyclass]
struct PyOpFn {
    callable: PyObject,
}

struct PyOperatorHandler {
    arena: PyObject,
    native_lookup: FLookup<IntAllocator>,
    //native_callable: HashMap<Vec<u8>, Box<OpFn>>,
    py_callable: HashMap<Vec<u8>, PyObject>,
}

impl OperatorHandler<IntAllocator> for PyOperatorHandler {
    fn op(
        &self,
        allocator: &mut IntAllocator,
        op_buf: <IntAllocator as Allocator>::AtomBuf,
        args: &<IntAllocator as Allocator>::Ptr,
        max_cost: Cost,
    ) -> Response<<IntAllocator as Allocator>::Ptr> {
        let op = allocator.buf(&op_buf);
        if op.len() == 1 {
            if let Some(f) = self.native_lookup[op[0] as usize] {
                return f(allocator, args.clone(), max_cost);
            }
        }

        let py_obj = self.py_callable.get(op);
        if let Some(obj) = py_obj {
            return self.invoke_py_obj(obj.clone(), allocator, op_buf, args, max_cost);
        };

        Ok(Reduction(0, 0))
    }
}

impl PyOperatorHandler {
    fn invoke_py_obj(
        &self,
        obj: PyObject,
        allocator: &mut IntAllocator,
        op_buf: <IntAllocator as Allocator>::AtomBuf,
        args: &<IntAllocator as Allocator>::Ptr,
        max_cost: Cost,
    ) -> Response<<IntAllocator as Allocator>::Ptr> {
        Python::with_gil(|py| {
            let op = allocator.buf(&op_buf).to_object(py);
            let py_int_node: &PyCell<PyIntNode> =
                PyCell::new(py, PyIntNode::new(self.arena.clone(), Some(*args), None)).unwrap();

            // this hack ensures we have python representations in all children
            PyIntNode::ensure_python_view(vec![py_int_node.to_object(py)], allocator, py).unwrap();
            let r1 = obj.call1(py, (op, py_int_node));

            match r1 {
                Err(pyerr) => {
                    let eval_err: PyResult<EvalErr<i32>> = eval_err_for_pyerr(py, &pyerr);
                    let r: EvalErr<i32> =
                        unwrap_or_eval_err(eval_err, args, "unexpected exception")?;
                    Err(r)
                }
                Ok(o) => {
                    let pair: &PyTuple = unwrap_or_eval_err(o.extract(py), args, "expected tuple")?;

                    let i0: u32 =
                        unwrap_or_eval_err(pair.get_item(0).extract(), args, "expected u32")?;

                    let py_node: &PyCell<PyIntNode> =
                        unwrap_or_eval_err(pair.get_item(1).extract(), args, "expected node")?;

                    let node: i32 = PyIntNode::ptr(py_node, Some(py));
                    Ok(Reduction(i0 as Cost, node))
                }
            }
        })
    }
}

//fn(&mut T, <T as Allocator>::Ptr, Cost) -> Response<<T as Allocator>::Ptr>;

/// turn a `PyErr` into an `EvalErr<P>` if at all possible
/// otherwise, return a `PyErr`

fn eval_err_for_pyerr<'p>(py: Python<'p>, pyerr: &PyErr) -> PyResult<EvalErr<i32>> {
    let args: &PyTuple = pyerr.pvalue(py).getattr("args")?.extract()?;
    let arg0: &PyString = args.get_item(0).extract()?;
    let sexp: &PyCell<PyIntNode> = pyerr.pvalue(py).getattr("_sexp")?.extract()?;
    let node: i32 = PyIntNode::ptr(&sexp, Some(py));
    let s: String = arg0.to_str()?.to_string();
    Ok(EvalErr(node, s))
}

fn unwrap_or_eval_err<T, P>(obj: PyResult<T>, err_node: &P, msg: &str) -> Result<T, EvalErr<P>>
where
    P: Clone,
{
    match obj {
        Err(_py_err) => Err(EvalErr(err_node.clone(), msg.to_string())),
        Ok(o) => Ok(o),
    }
}
