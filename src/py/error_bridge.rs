use crate::allocator::{Allocator, NodePtr};
use crate::reduction::EvalErr;
use pyo3::types::{PyDict, PyString, PyTuple};
use pyo3::{PyAny, PyErr, PyObject, PyRef, PyResult, Python};

use super::arena::Arena;

/// turn a `PyErr` into an `EvalErr` if at all possible
/// otherwise, return a `PyErr`
pub fn eval_err_for_pyerr<'p>(
    py: Python<'p>,
    pyerr: &PyErr,
    arena: &'p PyRef<Arena>,
    allocator: &mut Allocator,
) -> PyResult<EvalErr> {
    let args: &PyTuple = pyerr.pvalue(py).getattr("args")?.extract()?;
    let arg0: &PyString = args.get_item(0).extract()?;
    let sexp: &PyAny = pyerr.pvalue(py).getattr("_sexp")?.extract()?;
    let node: i32 = Arena::native_for_py(arena, py, sexp, allocator)?;
    let s: String = arg0.to_str()?.to_string();
    Ok(EvalErr(node, s))
}

pub fn unwrap_or_eval_err<T>(obj: PyResult<T>, err_node: NodePtr, msg: &str) -> Result<T, EvalErr> {
    match obj {
        Err(_py_err) => Err(EvalErr(err_node, msg.to_string())),
        Ok(o) => Ok(o),
    }
}

pub fn raise_eval_error(py: Python, msg: &PyString, sexp: PyObject) -> PyResult<PyObject> {
    let ctx: &PyDict = PyDict::new(py);
    ctx.set_item("msg", msg)?;
    ctx.set_item("sexp", sexp)?;
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
