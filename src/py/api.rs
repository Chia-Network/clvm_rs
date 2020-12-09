use super::arc_allocator::ArcAllocator;
use super::native_op_lookup::NativeOpLookup;
use super::node::Node;
use crate::allocator::NodeT;
use crate::reduction::{EvalErr, Reduction};
use crate::run_program::run_program;
use crate::serialize::{node_from_bytes, node_to_bytes};
use crate::tracing::{PostEval, PreEval};
use crate::types::OperatorHandler;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};
use pyo3::wrap_pyfunction;
use pyo3::PyObject;

impl From<PyErr> for EvalErr<Node> {
    fn from(_err: PyErr) -> Self {
        let pyerr_node: Node = ArcAllocator::new().blob("PyErr");
        EvalErr(pyerr_node, "bad type from python call".to_string())
    }
}

fn note_result(obj: &PyObject, result: Option<&Node>) {
    Python::with_gil(|py| {
        if let Some(node) = result {
            let _r: PyResult<PyObject> = obj.call1(py, (node.clone(),));
        }
    });
}

fn post_eval_for_pyobject(obj: PyObject) -> Option<Box<PostEval<Node>>> {
    let py_post_eval: Option<Box<PostEval<Node>>> = if Python::with_gil(|py| obj.is_none(py)) {
        None
    } else {
        Some(Box::new(move |result: Option<&Node>| {
            note_result(&obj, result)
        }))
    };
    py_post_eval
}

#[pyfunction]
fn py_run_program(
    py: Python,
    program: &Node,
    args: &Node,
    quote_kw: u8,
    max_cost: u32,
    op_lookup: NativeOpLookup,
    pre_eval: PyObject,
) -> PyResult<(u32, Node)> {
    let allocator = ArcAllocator::new();
    let py_pre_eval_t: Option<PreEval<Node>> = if pre_eval.is_none(py) {
        None
    } else {
        Some(Box::new(move |program: &Node, args: &Node| {
            Python::with_gil(|py| {
                let program_clone: Node = program.clone();
                let args: Node = args.clone();
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

    let allocator: ArcAllocator = ArcAllocator::new();
    let f: OperatorHandler<Node> =
        Box::new(move |allocator, op, args| op_lookup.operator_handler(allocator, op, args));

    let r: Result<Reduction<Node>, EvalErr<Node>> = run_program(
        &allocator,
        &program,
        &args,
        quote_kw,
        max_cost,
        &f,
        py_pre_eval_t,
    );
    match r {
        Ok(reduction) => Ok((reduction.0, reduction.1)),
        Err(eval_err) => {
            let node: Node = eval_err.0;
            let s: String = eval_err.1;
            let s1: &str = &s;
            let msg: &PyString = PyString::new(py, s1);
            let sexp_any: Node = node;
            match raise_eval_error(py, &msg, &sexp_any) {
                Err(x) => Err(x),
                _ => panic!(),
            }
        }
    }
}

#[pyfunction]
fn raise_eval_error(py: Python, msg: &PyString, sexp: &Node) -> PyResult<PyObject> {
    let local_sexp: Node = sexp.clone();
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
fn serialize_from_bytes(blob: &[u8]) -> PyResult<Node> {
    let allocator: ArcAllocator = ArcAllocator::new();
    let node = node_from_bytes(&allocator, blob).unwrap();
    Ok(node)
}

#[pyfunction]
fn serialize_to_bytes(py: Python, sexp: &Node) -> PyResult<PyObject> {
    let allocator: ArcAllocator = ArcAllocator::new();
    let node_t: NodeT<Node> = NodeT::new(&allocator, sexp.clone());
    let blob = node_to_bytes(&node_t).unwrap();
    let pybytes = PyBytes::new(py, &blob);
    let pyany: PyObject = pybytes.to_object(py);
    Ok(pyany)
}

/// This module is a python module implemented in Rust.
#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_run_program, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_from_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_to_bytes, m)?)?;

    m.add_class::<Node>()?;
    m.add_class::<NativeOpLookup>()?;

    m.add_function(wrap_pyfunction!(raise_eval_error, m)?)?;

    Ok(())
}
