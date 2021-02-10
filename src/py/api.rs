use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
use pyo3::wrap_pyfunction;
use pyo3::PyObject;

use super::arc_allocator::ArcAllocator;
use super::f_table::make_f_lookup;
use super::glue::{_py_run_program, _serialize_from_bytes, _serialize_to_bytes};
use super::native_op_lookup::GenericNativeOpLookup;
use super::py_node::PyNode;
use super::run_program::{__pyo3_get_function_serialize_and_run_program, STRICT_MODE};

type AllocatorT = ArcAllocator;
type NodeClass = PyNode;

#[pyclass]
#[derive(Clone)]
pub struct NativeOpLookup {
    nol: GenericNativeOpLookup<AllocatorT>,
}

#[pymethods]
impl NativeOpLookup {
    #[new]
    fn new(native_opcode_list: &[u8], unknown_op_callback: PyObject) -> Self {
        let native_lookup = make_f_lookup();
        let mut f_lookup = [None; 256];
        for i in native_opcode_list.iter() {
            let idx = *i as usize;
            f_lookup[idx] = native_lookup[idx];
        }
        NativeOpLookup {
            nol: GenericNativeOpLookup::new(unknown_op_callback, f_lookup),
        }
    }
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn py_run_program(
    py: Python,
    program: &NodeClass,
    args: &NodeClass,
    quote_kw: u8,
    apply_kw: u8,
    max_cost: u32,
    op_lookup: NativeOpLookup,
    pre_eval: PyObject,
) -> PyResult<(u32, NodeClass)> {
    let allocator = AllocatorT::new();
    _py_run_program(
        py,
        &allocator,
        program,
        args,
        quote_kw,
        apply_kw,
        max_cost,
        op_lookup.nol,
        pre_eval,
    )
}

#[pyfunction]
fn raise_eval_error(py: Python, msg: &PyString, sexp: PyObject) -> PyResult<PyObject> {
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

#[pyfunction]
fn serialize_from_bytes(blob: &[u8]) -> NodeClass {
    _serialize_from_bytes(&AllocatorT::default(), blob)
}

#[pyfunction]
fn serialize_to_bytes(py: Python, sexp: &PyAny) -> PyResult<PyObject> {
    _serialize_to_bytes::<AllocatorT, NodeClass>(&AllocatorT::default(), py, sexp)
}

/// This module is a python module implemented in Rust.
#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_run_program, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_and_run_program, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_from_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_to_bytes, m)?)?;

    m.add("STRICT_MODE", STRICT_MODE)?;

    m.add_class::<PyNode>()?;
    m.add_class::<NativeOpLookup>()?;

    m.add_function(wrap_pyfunction!(raise_eval_error, m)?)?;

    Ok(())
}
