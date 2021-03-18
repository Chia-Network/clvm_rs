use std::collections::HashMap;

use pyo3::types::PyBytes;
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
use crate::reduction::{EvalErr, Reduction, Response};
use crate::run_program::OperatorHandler;

use super::f_table::{f_lookup_for_hashmap, FLookup};
use super::py_int_allocator::PyIntAllocator;
use super::py_node::PyNode;

pub struct PyOperatorHandler<'p> {
    native_lookup: FLookup<IntAllocator>,
    py_callable: PyObject,
    py_int_allocator: &'p PyIntAllocator,
}

impl<'p> PyOperatorHandler<'p> {
    pub fn new(
        opcode_lookup_by_name: HashMap<String, Vec<u8>>,
        py_callable: PyObject,
        py_int_allocator: &'p PyIntAllocator,
    ) -> PyResult<Self> {
        let native_lookup = f_lookup_for_hashmap(opcode_lookup_by_name);
        Ok(PyOperatorHandler {
            native_lookup,
            py_callable,
            py_int_allocator,
        })
    }

    pub fn invoke_py_obj(
        &self,
        obj: PyObject,
        allocator: &mut IntAllocator,
        op_buf: <IntAllocator as Allocator>::AtomBuf,
        args: &<IntAllocator as Allocator>::Ptr,
        max_cost: Cost,
    ) -> Response<<IntAllocator as Allocator>::Ptr> {
        Python::with_gil(|py| {
            let op: &PyBytes = PyBytes::new(py, allocator.buf(&op_buf));
            let r = unwrap_or_eval_err(
                self.py_int_allocator.py_for_native(py, args, allocator),
                args,
                "can't uncache",
            )?;
            let r1 = obj.call1(py, (op, r.to_object(py), max_cost));
            match r1 {
                Err(pyerr) => {
                    let eval_err: PyResult<EvalErr<i32>> =
                        eval_err_for_pyerr(py, &pyerr, self.py_int_allocator, allocator);
                    let r: EvalErr<i32> =
                        unwrap_or_eval_err(eval_err, args, "unexpected exception")?;
                    Err(r)
                }
                Ok(o) => {
                    let pair: &PyTuple = unwrap_or_eval_err(o.extract(py), args, "expected tuple")?;

                    let i0: u32 =
                        unwrap_or_eval_err(pair.get_item(0).extract(), args, "expected u32")?;

                    let py_node: &PyCell<PyNode> =
                        unwrap_or_eval_err(pair.get_item(1).extract(), args, "expected node")?;

                    let r = self.py_int_allocator.native_for_py(py, py_node, allocator);
                    let node: i32 = unwrap_or_eval_err(r, args, "can't find in int allocator")?;
                    Ok(Reduction(i0 as Cost, node))
                }
            }
        })
    }
}

impl OperatorHandler<IntAllocator> for PyOperatorHandler<'_> {
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
                return f(allocator, *args, max_cost);
            }
        }

        self.invoke_py_obj(self.py_callable.clone(), allocator, op_buf, args, max_cost)
    }
}

/// turn a `PyErr` into an `EvalErr<P>` if at all possible
/// otherwise, return a `PyErr`
fn eval_err_for_pyerr<'p>(
    py: Python<'p>,
    pyerr: &PyErr,
    py_int_allocator: &'p PyIntAllocator,
    allocator: &mut IntAllocator,
) -> PyResult<EvalErr<i32>> {
    let args: &PyTuple = pyerr.pvalue(py).getattr("args")?.extract()?;
    let arg0: &PyString = args.get_item(0).extract()?;
    let sexp: &PyCell<PyNode> = pyerr.pvalue(py).getattr("_sexp")?.extract()?;
    let node: i32 = py_int_allocator.native_for_py(py, sexp, allocator)?;
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
