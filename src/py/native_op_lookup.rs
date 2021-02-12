use std::marker::PhantomData;

use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};
use pyo3::PyClass;

use crate::allocator::Allocator;
use crate::reduction::{EvalErr, Reduction, Response};
use crate::run_program::OperatorHandler;

use super::f_table::FLookup;
use super::to_py_node::ToPyNode;

fn eval_err_for_pyerr<'s, 'p: 's, 'e: 's, P, N>(
    py: Python<'p>,
    pyerr: &'e PyErr,
) -> PyResult<EvalErr<P>>
where
    P: From<N>,
    N: FromPyObject<'s>,
{
    let args: &PyTuple = pyerr.pvalue(py).getattr("args")?.extract()?;
    let arg0: &PyString = args.get_item(0).extract()?;
    let sexp: N = pyerr.pvalue(py).getattr("_sexp")?.extract()?;
    let node: P = sexp.into();
    let s: String = arg0.to_str()?.to_string();
    Ok(EvalErr(node, s))
}
#[derive(Clone)]
pub struct GenericNativeOpLookup<A, N>
where
    A: 'static + Allocator + ToPyNode<N>,
    N: PyClass,
    <A as Allocator>::Ptr: From<N>,
{
    py_callback: PyObject,
    f_lookup: FLookup<A>,
    phantom_data: PhantomData<N>,
}

impl<A, N> GenericNativeOpLookup<A, N>
where
    A: 'static + Allocator + ToPyNode<N>,
    N: PyClass,
    <A as Allocator>::Ptr: From<N>,
{
    pub fn new(py_callback: PyObject, f_lookup: FLookup<A>) -> Self {
        GenericNativeOpLookup {
            py_callback,
            f_lookup,
            phantom_data: PhantomData,
        }
    }
}

impl<A, N> OperatorHandler<A> for GenericNativeOpLookup<A, N>
where
    A: 'static + Allocator + ToPyNode<N>,
    N: PyClass + Clone + IntoPy<PyObject>,
    <A as Allocator>::Ptr: From<N>,
{
    fn op(
        &self,
        allocator: &A,
        op: &[u8],
        argument_list: &<A as Allocator>::Ptr,
    ) -> Response<<A as Allocator>::Ptr> {
        eval_op::<A, N>(
            &self.f_lookup,
            &self.py_callback,
            allocator,
            op,
            argument_list,
        )
    }
}

fn eval_op<A, N>(
    f_lookup: &FLookup<A>,
    py_callback: &PyObject,
    allocator: &A,
    op: &[u8],
    argument_list: &<A as Allocator>::Ptr,
) -> Response<<A as Allocator>::Ptr>
where
    A: Allocator + ToPyNode<N>,
    <A as Allocator>::Ptr: From<N>,
    N: PyClass + Clone,
    N: IntoPy<PyObject>,
{
    if op.len() == 1 {
        if let Some(f) = f_lookup[op[0] as usize] {
            return f(&allocator, argument_list.clone());
        }
    }

    Python::with_gil(|py| {
        let pynode: N = allocator.to_pynode(argument_list);
        let r1 = py_callback.call1(py, (op, pynode));
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

                let i0: u32 =
                    unwrap_or_eval_err(pair.get_item(0).extract(), argument_list, "expected u32")?;

                let py_node: N =
                    unwrap_or_eval_err(pair.get_item(1).extract(), argument_list, "expected node")?;

                let node: <A as Allocator>::Ptr = py_node.into();
                Ok(Reduction(i0, node))
            }
        }
    })
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
