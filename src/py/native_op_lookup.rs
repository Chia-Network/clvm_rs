use crate::allocator::Allocator;
use crate::node::Node;
use crate::reduction::{EvalErr, Reduction};
use crate::run_program::OperatorHandler;

use super::arc_allocator::ArcAllocator;
use super::f_table::{make_f_lookup, FLookup};
use super::py_node::PyNode;

use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};

#[pyclass]
#[derive(Clone)]
pub struct NativeOpLookup {
    nol: GenericNativeOpLookup<ArcAllocator>,
}

#[pymethods]
impl NativeOpLookup {
    #[new]
    fn new(native_opcode_list: &[u8], unknown_op_callback: &PyAny) -> Self {
        let native_lookup = make_f_lookup();
        let mut f_lookup: FLookup<ArcAllocator> = [None; 256];
        for i in native_opcode_list.iter() {
            let idx = *i as usize;
            f_lookup[idx] = native_lookup[idx];
        }
        NativeOpLookup {
            nol: GenericNativeOpLookup {
                py_callback: unknown_op_callback.into(),
                f_lookup,
            },
        }
    }
}

fn eval_err_for_pyerr(py: Python, pyerr: &PyErr) -> PyResult<EvalErr<ArcSExp>> {
    let args: &PyTuple = pyerr.pvalue(py).getattr("args")?.extract()?;
    let arg0: &PyString = args.get_item(0).extract()?;
    let sexp: &PyCell<PyNode> = pyerr.pvalue(py).getattr("_sexp")?.extract()?;

    let sexp_ptr: PyRef<PyNode> = sexp.try_borrow()?;
    let node: ArcSExp = (&sexp_ptr as &PyNode).into();
    let s: String = arg0.to_str()?.to_string();
    Ok(EvalErr(node, s))
}

impl NativeOpLookup {
    pub fn make_operator_handler(self) -> OperatorHandler<ArcAllocator> {
        Box::new(move |allocator, op, args| self.nol.operator_handler(allocator, op, args))
    }
}
#[derive(Clone)]
struct GenericNativeOpLookup<A: Allocator> {
    py_callback: PyObject,
    f_lookup: FLookup<A>,
}

impl<A: Allocator> GenericNativeOpLookup<A> {
    pub fn operator_handler<'t>(
        &self,
        allocator: &A,
        op: &[u8],
        argument_list: &'t <A as Allocator>::Ptr,
    ) -> Result<Reduction<<A as Allocator>::Ptr>, EvalErr<<A as Allocator>::Ptr>>
    where
        <A as Allocator>::Ptr: From<PyNode>,
        PyNode: From<&'t <A as Allocator>::Ptr>,
    {
        if op.len() == 1 {
            if let Some(f) = self.f_lookup[op[0] as usize] {
                let node_t: Node<A> = Node::new(allocator, argument_list.clone());
                return f(&node_t);
            }
        }

        Python::with_gil(|py| {
            let pynode: PyNode = argument_list.into();
            let r1 = self.py_callback.call1(py, (op, pynode));
            match r1 {
                Err(pyerr) => {
                    let ee = eval_err_for_pyerr(py, &pyerr);
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

                    let py_node: PyNode = unwrap_or_eval_err(
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
