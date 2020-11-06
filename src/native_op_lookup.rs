use super::node::Node;
use super::pysexp::PySExp;
use super::types::{EvalErr, OperatorFT, Reduction};

use std::collections::HashMap;

use super::f_table::{make_f_lookup, FLookup};

pub struct Pair(Vec<u8>, Box<dyn OperatorFT>);

struct OpLookupHash {
    map: HashMap<Vec<u8>, Box<dyn OperatorFT>>,
}

impl OpLookupHash {
    fn new(pairs: Vec<Pair>) -> OpLookupHash {
        let mut map: HashMap<Vec<u8>, Box<dyn OperatorFT>> = HashMap::new();
        for pair in pairs.into_iter() {
            let name = pair.0;
            let func = pair.1;
            map.insert(name, func);
        }
        OpLookupHash { map }
    }
}

//use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

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

impl NativeOpLookup {
    pub fn operator_handler(&self, op: &[u8], argument_list: &Node) -> Result<Reduction, EvalErr> {
        if op.len() == 1 {
            if let Some(f) = self.f_lookup[op[0] as usize] {
                return f(argument_list);
            }
        }

        Python::with_gil(|py| {
            let pysexp: PySExp = argument_list.clone().into();
            let r1 = self.py_callback.call1(py, (op, pysexp));
            match r1 {
                Err(_) => argument_list.err("fooooooooo"),
                Ok(o) => {
                    let pair: &PyTuple = o.extract(py).unwrap();
                    let i0: u32 = pair.get_item(0).extract()?;
                    let i1: PyRef<PySExp> = pair.get_item(1).extract()?;
                    let n = i1.node.clone();
                    let r: Reduction = Reduction(n, i0);
                    Ok(r)
                }
            }
        })
    }
}

/*
impl From<&PyAny> for NativeOpLookup {
    fn from(item: &PyAny) -> NativeOpLookup {
        let t: PyResult<&NativeOpLookup> = item.extract();
        if let Ok(nop) = t {
            return nop.clone();
        }
        let empty: [u8; 0] = [];
        NativeOpLookup::new(&empty, item)
    }
}
*/


/*

impl IntoPy<NativeOpLookup> for Node {
    fn into_py(self, _py: Python<'_>) -> PySExp {
        PySExp { node: self }
    }
}

*/

/*
fn extract_atom(obj: &PyAny) -> PyResult<Node> {
    let r: &[u8] = obj.extract()?;
    Ok(Node::blob_u8(r))
}

fn extract_node(obj: &PyAny) -> PyResult<Node> {
    let ps: PyRef<PySExp> = obj.extract()?;
    let node: Node = ps.node.clone();
    Ok(node)
}

fn extract_tuple(obj: &PyAny) -> PyResult<Node> {
    let v: &PyTuple = obj.extract()?;
    if v.len() != 2 {
        return Err(PyValueError::new_err("SExp tuples must be size 2"));
    }
    let i0: &PyAny = v.get_item(0);
    let i1: &PyAny = v.get_item(1);
    let left: Node = extract_node(i0)?;
    let right: Node = extract_node(i1)?;
    let node: Node = Node::pair(&left, &right);
    Ok(node)
}
*/
