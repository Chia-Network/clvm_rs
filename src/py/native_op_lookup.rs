use super::f_table::FLookup;
use crate::allocator::Allocator;
use crate::node::Node;
use crate::reduction::{EvalErr, Reduction};

use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};
use pyo3::PyClass;

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
pub struct GenericNativeOpLookup<A: Allocator> {
    py_callback: PyObject,
    f_lookup: FLookup<A>,
}

impl<A: Allocator> GenericNativeOpLookup<A> {
    pub fn new(py_callback: PyObject, f_lookup: FLookup<A>) -> Self {
        GenericNativeOpLookup {
            py_callback,
            f_lookup,
        }
    }

    pub fn operator_handler<'t, N>(
        &self,
        allocator: &A,
        op: &[u8],
        argument_list: &'t <A as Allocator>::Ptr,
    ) -> Result<Reduction<<A as Allocator>::Ptr>, EvalErr<<A as Allocator>::Ptr>>
    where
        <A as Allocator>::Ptr: From<N>,
        N: From<&'t <A as Allocator>::Ptr>,
        N: PyClass + Clone,
        N: IntoPy<PyObject>,
    {
        if op.len() == 1 {
            if let Some(f) = self.f_lookup[op[0] as usize] {
                let node_t: Node<A> = Node::new(allocator, argument_list.clone());
                return f(&node_t);
            }
        }

        Python::with_gil(|py| {
            let pynode: N = argument_list.into();
            let r1 = self.py_callback.call1(py, (op, pynode));
            match r1 {
                Err(pyerr) => {
                    let ee = eval_err_for_pyerr::<<A as Allocator>::Ptr, N>(py, &pyerr);
                    match ee {
                        Err(_x) => {
                            println!("{:?}", _x);
                            Err(EvalErr(argument_list.clone(), "internal error".to_string()))
                        }
                        Ok(ee) => Err(ee),
                    }
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
                    Ok(Reduction(i0, node))
                }
            }
        })
    }
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
