use std::collections::HashMap;

use pyo3::prelude::pyclass;
use pyo3::PyCell;
use pyo3::PyObject;
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
            /*
            match r1 {
                Err(pyerr) => {
                    let eval_err: PyResult<EvalErr<<A as Allocator>::Ptr>> =
                        eval_err_for_pyerr(py, &pyerr);
                    let r: EvalErr<<A as Allocator>::Ptr> =
                        unwrap_or_eval_err(eval_err, argument_list, "unexpected exception")?;
                    Err(r)
                }
                Ok(o) => {
                    let pair: &PyTuple =
                        unwrap_or_eval_err(o.extract(py), argument_list, "expected tuple")?;

                    let i0: u32 = unwrap_or_eval_err(
                        pair.get_item(0).extract(),
                        argument_list,
                        "expected u32",
                    )?;

                    let py_node: N = unwrap_or_eval_err(
                        pair.get_item(1).extract(),
                        argument_list,
                        "expected node",
                    )?;

                    let node: <A as Allocator>::Ptr = py_node.into();
                    Ok(Reduction(i0 as Cost, node))
                }
            }
            */
            Ok(Reduction(0, 0))
        })
    }
}

//fn(&mut T, <T as Allocator>::Ptr, Cost) -> Response<<T as Allocator>::Ptr>;
