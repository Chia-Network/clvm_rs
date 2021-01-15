use super::arc_allocator::ArcAllocator;
use super::native_op_lookup::NativeOpLookup;
use super::py_node::PyNode;
use crate::node::Node;
use crate::reduction::{EvalErr, Reduction};
use crate::run_program::{run_program, PostEval, PreEval};
use crate::serialize::{node_from_bytes, node_to_bytes};
use crate::types::OperatorHandler;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};
use pyo3::wrap_pyfunction;
use pyo3::PyObject;

impl From<PyErr> for EvalErr<PyNode> {
    fn from(_err: PyErr) -> Self {
        let pyerr_node: PyNode = ArcAllocator::new().blob("PyErr");
        EvalErr(pyerr_node, "bad type from python call".to_string())
    }
}

fn note_result(obj: &PyObject, result: Option<&PyNode>) {
    Python::with_gil(|py| {
        if let Some(node) = result {
            let _r: PyResult<PyObject> = obj.call1(py, (node.clone(),));
        }
    });
}

fn post_eval_for_pyobject(obj: PyObject) -> Option<Box<PostEval<PyNode>>> {
    let py_post_eval: Option<Box<PostEval<PyNode>>> = if Python::with_gil(|py| obj.is_none(py)) {
        None
    } else {
        Some(Box::new(move |result: Option<&PyNode>| {
            note_result(&obj, result)
        }))
    };
    py_post_eval
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn py_run_program(
    py: Python,
    program: &PyNode,
    args: &PyNode,
    quote_kw: u8,
    apply_kw: u8,
    max_cost: u32,
    op_lookup: NativeOpLookup,
    pre_eval: PyObject,
) -> PyResult<(u32, PyNode)> {
    let allocator = ArcAllocator::new();
    let py_pre_eval_t: Option<PreEval<PyNode>> = if pre_eval.is_none(py) {
        None
    } else {
        Some(Box::new(move |program: &PyNode, args: &PyNode| {
            Python::with_gil(|py| {
                let program_clone: PyNode = program.clone();
                let args: PyNode = args.clone();
                let r: PyResult<PyObject> = pre_eval.call1(py, (program_clone, args));
                match r {
                    Ok(py_post_eval) => {
                        let f = post_eval_for_pyobject(py_post_eval);
                        Ok(f)
                    }
                    Err(ref err) => allocator.err(program, &err.to_string()),
                }
            })
        }))
    };

    // BRAIN DAMAGE: we create a second `ArcAllocator` here
    // This only works because this allocator type has the property that
    // you can create a pair from nodes from different allocators.

    let allocator: ArcAllocator = ArcAllocator::new();
    let f: OperatorHandler<PyNode> =
        Box::new(move |allocator, op, args| op_lookup.operator_handler(allocator, op, args));

    let r: Result<Reduction<PyNode>, EvalErr<PyNode>> = run_program(
        &allocator,
        &program,
        &args,
        quote_kw,
        apply_kw,
        max_cost,
        &f,
        py_pre_eval_t,
    );
    match r {
        Ok(reduction) => Ok((reduction.0, reduction.1)),
        Err(eval_err) => {
            let node: PyNode = eval_err.0;
            let s: String = eval_err.1;
            let s1: &str = &s;
            let msg: &PyString = PyString::new(py, s1);
            let sexp_any: PyNode = node;
            match raise_eval_error(py, &msg, &sexp_any) {
                Err(x) => Err(x),
                _ => panic!(),
            }
        }
    }
}

#[pyfunction]
fn raise_eval_error(py: Python, msg: &PyString, sexp: &PyNode) -> PyResult<PyObject> {
    let local_sexp: PyNode = sexp.clone();
    let sexp_any: PyObject = local_sexp.into_py(py);
    let msg_any: PyObject = msg.into_py(py);

    let s0: &PyString = PyString::new(py, "msg");
    let s1: &PyString = PyString::new(py, "sexp");
    let ctx: &PyDict = PyDict::new(py);
    ctx.set_item(s0, msg_any)?;
    ctx.set_item(s1, sexp_any)?;

    let r = py.run(
        "from clvm.EvalError import EvalError; raise EvalError(msg, sexp)",
        None,
        Some(ctx),
    );
    match r {
        Err(x) => Err(x),
        Ok(_) => Ok(ctx.into()),
    }
}

#[pyfunction]
fn serialize_from_bytes(blob: &[u8]) -> PyNode {
    let allocator: ArcAllocator = ArcAllocator::new();
    node_from_bytes(&allocator, blob).unwrap()
}

#[pyfunction]
fn serialize_to_bytes(py: Python, sexp: &PyNode) -> PyObject {
    let allocator: ArcAllocator = ArcAllocator::new();
    let node_t: Node<PyNode> = Node::new(&allocator, sexp.clone());
    let blob = node_to_bytes(&node_t).unwrap();
    let pybytes = PyBytes::new(py, &blob);
    pybytes.to_object(py)
}

/// This module is a python module implemented in Rust.
#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_run_program, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_from_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_to_bytes, m)?)?;

    m.add_class::<PyNode>()?;
    m.add_class::<NativeOpLookup>()?;

    m.add_function(wrap_pyfunction!(raise_eval_error, m)?)?;

    Ok(())
}
