use crate::allocator::{Allocator, NodeT};
use crate::node::Node;
use crate::reduction::Reduction;
use crate::types::EvalErr;

use crate::f_table::{make_f_lookup, FLookup};

use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};

#[pyclass]
#[derive(Clone)]
pub struct NativeOpLookup {
    py_callback: PyObject,
    f_lookup: FLookup,
}

#[pymethods]
impl NativeOpLookup {
    #[new]
    fn new(native_opcode_list: &[u8], unknown_op_callback: &PyAny) -> Self {
        let native_lookup = make_f_lookup();
        let mut f_lookup: FLookup = [None; 256];
        for i in native_opcode_list.iter() {
            let idx = *i as usize;
            f_lookup[idx] = native_lookup[idx];
        }
        NativeOpLookup {
            py_callback: unknown_op_callback.into(),
            f_lookup,
        }
    }
}

fn eval_err_for_pyerr(py: Python, pyerr: &PyErr) -> PyResult<EvalErr<Node>> {
    let args: &PyTuple = pyerr.pvalue(py).getattr("args")?.extract()?;
    let arg0: &PyString = args.get_item(0).extract()?;
    let sexp: &PyCell<Node> = pyerr.pvalue(py).getattr("_sexp")?.extract()?;

    let node: Node = sexp.try_borrow()?.clone();
    let s: String = arg0.to_str()?.to_string();
    Ok(EvalErr(node, s))
}

impl NativeOpLookup {
    pub fn operator_handler(
        &self,
        allocator: &dyn Allocator<Node>,
        op: &[u8],
        argument_list: &Node,
    ) -> Result<Reduction<Node>, EvalErr<Node>> {
        if op.len() == 1 {
            if let Some(f) = self.f_lookup[op[0] as usize] {
                let node_t: NodeT<Node> = NodeT::new(allocator, argument_list.clone());
                return f(&node_t);
            }
        }

        Python::with_gil(|py| {
            let pysexp: Node = argument_list.clone();
            let r1 = self.py_callback.call1(py, (op, pysexp));
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
                    let pair: &PyTuple = o.extract(py).unwrap();
                    let i0: u32 = pair.get_item(0).extract()?;
                    let i1: PyRef<Node> = pair.get_item(1).extract()?;
                    let n = i1.clone();
                    let r: Reduction<Node> = Reduction(i0, n);
                    Ok(r)
                }
            }
        })
    }
}
