use super::native_op_lookup::NativeOpLookup;
use super::node::Node;
use super::pysexp::PySExp;
use super::run_program::run_program;
use super::serialize::{node_from_bytes, node_to_bytes};
use super::types::{EvalErr, OperatorHandler, PostEval, PreEval, Reduction};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};
use pyo3::wrap_pyfunction;
use pyo3::PyObject;

impl From<PyErr> for EvalErr {
    fn from(_err: PyErr) -> Self {
        EvalErr(Node::blob("PyErr"), "bad type from python call".to_string())
    }
}

fn note_result(obj: &PyObject, result: Option<&Node>) {
    Python::with_gil(|py| {
        if let Some(node) = result {
            let py_sexp: PySExp = node.clone().into();
            let _r: PyResult<PyObject> = obj.call1(py, (py_sexp,));
        }
    });
}

fn post_eval_for_pyobject(obj: PyObject) -> Option<Box<PostEval>> {
    let py_post_eval: Option<Box<PostEval>> = if Python::with_gil(|py| obj.is_none(py)) {
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
    program: &PySExp,
    args: &PySExp,
    quote_kw: u8,
    max_cost: u32,
    op_lookup: NativeOpLookup,
    pre_eval: PyObject,
) -> PyResult<(u32, PySExp)> {
    let py_pre_eval_t: Option<PreEval> = if pre_eval.is_none(py) {
        None
    } else {
        Some(Box::new(move |program: &Node, args: &Node| {
            Python::with_gil(|py| {
                let prog_sexp: PySExp = program.clone().into_py(py);
                let args_sexp: PySExp = args.clone().into_py(py);
                let r: PyResult<PyObject> = pre_eval.call1(py, (prog_sexp, args_sexp));
                match r {
                    Ok(py_post_eval) => {
                        let f = post_eval_for_pyobject(py_post_eval);
                        Ok(f)
                    }
                    Err(ref err) => program.err(&err.to_string()),
                }
            })
        }))
    };

    let f: OperatorHandler = Box::new(move |op, args| op_lookup.operator_handler(op, args));

    let r: Result<Reduction, EvalErr> = run_program(
        &program.node,
        &args.node,
        quote_kw,
        max_cost,
        &f,
        py_pre_eval_t,
    );
    match r {
        Ok(reduction) => Ok((reduction.1, reduction.0.into())),
        Err(eval_err) => {
            let node: Node = eval_err.0;
            let s: String = eval_err.1;
            let s1: &str = &s;
            let msg: &PyString = PyString::new(py, s1);
            let sexp_any: PySExp = PySExp { node };
            match raise_eval_error(py, &msg, &sexp_any) {
                Err(x) => Err(x),
                _ => panic!(),
            }
        }
    }
    // TODO: recast to same type as `program`
}

#[pyfunction]
fn raise_eval_error(py: Python, msg: &PyString, sexp: &PySExp) -> PyResult<PyObject> {
    let local_sexp = PySExp {
        node: sexp.node.clone(),
    };
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
fn serialize_from_bytes(blob: &[u8]) -> PyResult<PySExp> {
    let node = node_from_bytes(blob).unwrap();
    Ok(node.into())
}

#[pyfunction]
fn serialize_to_bytes(py: Python, sexp: &PySExp) -> PyResult<PyObject> {
    let blob = node_to_bytes(&sexp.node).unwrap();
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

    m.add_class::<PySExp>()?;
    m.add_class::<NativeOpLookup>()?;

    m.add_function(wrap_pyfunction!(raise_eval_error, m)?)?;

    Ok(())
}
