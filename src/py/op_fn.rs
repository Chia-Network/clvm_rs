use std::collections::HashMap;

use pyo3::types::PyBytes;
use pyo3::types::PyTuple;
use pyo3::PyObject;
use pyo3::PyResult;
use pyo3::Python;
use pyo3::ToPyObject;
use pyo3::{PyAny, PyCell};

use crate::allocator::Allocator;
use crate::cost::Cost;
use crate::int_allocator::IntAllocator;
use crate::reduction::{EvalErr, Reduction, Response};
use crate::run_program::OperatorHandler;

use super::error_bridge::{eval_err_for_pyerr, unwrap_or_eval_err};
use super::f_table::{f_lookup_for_hashmap, FLookup};
use super::py_arena::PyArena;

pub struct PyOperatorHandler<'p> {
    native_lookup: FLookup<IntAllocator>,
    py_callable: PyObject,
    arena: &'p PyCell<PyArena>,
}

impl<'p> PyOperatorHandler<'p> {
    pub fn new(
        opcode_lookup_by_name: HashMap<String, Vec<u8>>,
        py_callable: PyObject,
        arena: &'p PyCell<PyArena>,
    ) -> PyResult<Self> {
        let native_lookup = f_lookup_for_hashmap(opcode_lookup_by_name);
        Ok(PyOperatorHandler {
            native_lookup,
            py_callable,
            arena,
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
                PyArena::py_for_native(&self.arena.borrow(), py, args, allocator),
                args,
                "can't uncache",
            )?;
            let r1 = obj.call1(py, (op, r.to_object(py), max_cost));
            match r1 {
                Err(pyerr) => {
                    let eval_err: PyResult<EvalErr<i32>> =
                        eval_err_for_pyerr(py, &pyerr, self.arena, allocator);
                    let r: EvalErr<i32> =
                        unwrap_or_eval_err(eval_err, args, "unexpected exception")?;
                    Err(r)
                }
                Ok(o) => {
                    let pair: &PyTuple = unwrap_or_eval_err(o.extract(py), args, "expected tuple")?;

                    let i0: u32 =
                        unwrap_or_eval_err(pair.get_item(0).extract(), args, "expected u32")?;

                    let clvm_object: &PyAny =
                        unwrap_or_eval_err(pair.get_item(1).extract(), args, "expected node")?;

                    let r = PyArena::native_for_py(self.arena, py, clvm_object, allocator);
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
