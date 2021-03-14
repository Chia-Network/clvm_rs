use std::cell::RefCell;
use std::collections::HashMap;

use pyo3::types::PyBytes;
use pyo3::types::PyString;
use pyo3::types::PyTuple;
use pyo3::PyCell;
use pyo3::PyErr;
use pyo3::PyObject;
use pyo3::PyRefMut;
use pyo3::PyResult;
use pyo3::Python;
use pyo3::ToPyObject;

use crate::allocator::Allocator;
use crate::cost::Cost;
use crate::int_allocator::IntAllocator;
use crate::py::f_table::{f_lookup_for_hashmap, FLookup};
use crate::py::py_na_node::{add_to_cache, from_cache, PyNaNode};
use crate::reduction::{EvalErr, Reduction, Response};
use crate::run_program::OperatorHandler;

pub struct PyOperatorHandler {
    native_lookup: FLookup<IntAllocator>,
    arena: PyObject,
    py_callable: PyObject,
    cache: PyObject,
}

impl PyOperatorHandler {
    pub fn new(
        opcode_lookup_by_name: HashMap<String, Vec<u8>>,
        arena: PyObject,
        cache: PyObject,
        py_callable: PyObject,
    ) -> Self {
        let native_lookup = f_lookup_for_hashmap(opcode_lookup_by_name);

        PyOperatorHandler {
            native_lookup,
            arena,
            py_callable,
            cache,
        }
    }
}

impl PyOperatorHandler {
    pub fn invoke_py_obj(
        &self,
        obj: PyObject,
        arena: PyObject,
        allocator: &mut IntAllocator,
        op_buf: <IntAllocator as Allocator>::AtomBuf,
        args: &<IntAllocator as Allocator>::Ptr,
        max_cost: Cost,
    ) -> Response<<IntAllocator as Allocator>::Ptr> {
        Python::with_gil(|py| {
            let op: &PyBytes = PyBytes::new(py, allocator.buf(&op_buf));
            let r = self.uncache(py, args);
            let py_int_node = unwrap_or_eval_err(r, args, "can't uncache")?;
            let mut py_na_node: PyRefMut<PyNaNode> =
                unwrap_or_eval_err(py_int_node.extract(py), args, "can't convert")?;
            if py_na_node.py_view.is_none() {
                py_na_node.py_view = Some(unwrap_or_eval_err(
                    PyNaNode::py_view_for_allocator_ptr(py, &arena, allocator, args),
                    args,
                    "can't generate pyview",
                )?);
            }
            drop(py_na_node);
            // TODO: implement a `populate_python_view` that accepts the borrowed `allocator` above
            //  since the existing one will try to re-borrow it and fail
            // py_int_node.populate_python_view(py);
            let r1 = obj.call1(py, (op, py_int_node.clone()));

            match r1 {
                Err(pyerr) => {
                    let eval_err: PyResult<EvalErr<i32>> = eval_err_for_pyerr(
                        py,
                        &pyerr,
                        self.cache.clone(),
                        arena.clone(),
                        allocator,
                    );
                    let r: EvalErr<i32> =
                        unwrap_or_eval_err(eval_err, args, "unexpected exception")?;
                    Err(r)
                }
                Ok(o) => {
                    let pair: &PyTuple = unwrap_or_eval_err(o.extract(py), args, "expected tuple")?;

                    let i0: u32 =
                        unwrap_or_eval_err(pair.get_item(0).extract(), args, "expected u32")?;

                    let py_node: &PyCell<PyNaNode> =
                        unwrap_or_eval_err(pair.get_item(1).extract(), args, "expected node")?;

                    let r = PyNaNode::ptr(py_node, py, &self.cache, &self.arena, allocator);
                    let node: i32 = unwrap_or_eval_err(r, args, "can't find in int allocator")?;
                    Ok(Reduction(i0 as Cost, node))
                }
            }
        })
    }

    fn uncache<'p>(&'p self, py: Python<'p>, args: &i32) -> PyResult<PyObject> {
        let args = args.clone();
        Ok(match from_cache(py, &self.cache, args)? {
            Some(obj) => obj,
            None => PyNaNode::from_ptr(py, &self.arena, args.clone())?.to_object(py),
        })
    }
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
                return f(allocator, *args, max_cost);
            }
        }

        self.invoke_py_obj(
            self.py_callable.clone(),
            self.arena.clone(),
            allocator,
            op_buf,
            args,
            max_cost,
        )
    }
}

/// turn a `PyErr` into an `EvalErr<P>` if at all possible
/// otherwise, return a `PyErr`
fn eval_err_for_pyerr<'p>(
    py: Python<'p>,
    pyerr: &PyErr,
    cache: PyObject,
    arena: PyObject,
    allocator: &mut IntAllocator,
) -> PyResult<EvalErr<i32>> {
    let args: &PyTuple = pyerr.pvalue(py).getattr("args")?.extract()?;
    let arg0: &PyString = args.get_item(0).extract()?;
    let sexp: &PyCell<PyNaNode> = pyerr.pvalue(py).getattr("_sexp")?.extract()?;
    let node: i32 = PyNaNode::ptr(&sexp, py, &cache, &arena, allocator)?;
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
