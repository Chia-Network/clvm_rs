use std::collections::HashMap;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};
use pyo3::wrap_pyfunction;
use pyo3::PyObject;

use super::py_int_allocator::PyIntAllocator;
use super::py_na_node::PyNaNode;
use super::py_native_mapping::{native_for_py, new_mapping, py_for_native};
use super::run_program::{__pyo3_get_function_deserialize_and_run_program, STRICT_MODE};

use crate::int_allocator::IntAllocator;

use crate::cost::Cost;
use crate::serialize::{node_from_bytes, node_to_bytes};

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
fn serialize_from_bytes<'p>(py: Python<'p>, blob: &[u8]) -> PyResult<&'p PyCell<PyNaNode>> {
    let py_int_allocator = PyCell::new(py, PyIntAllocator::default())?;
    let allocator: &mut IntAllocator = &mut py_int_allocator.borrow_mut().arena;
    let cache = new_mapping(py)?;
    let ptr = node_from_bytes(allocator, blob)?;
    py_for_native(py, &cache, &ptr, allocator)
}

use crate::node::Node;

#[pyfunction]
fn serialize_to_bytes<'p>(py: Python<'p>, sexp: &PyCell<PyNaNode>) -> PyResult<&'p PyBytes> {
    let py_int_allocator_cell = PyCell::new(py, PyIntAllocator::default())?;
    let py_int_allocator: &mut PyIntAllocator = &mut py_int_allocator_cell.borrow_mut();
    let allocator: &mut IntAllocator = &mut py_int_allocator.arena;

    let mapping = new_mapping(py)?;

    let ptr = native_for_py(py, &mapping, sexp, allocator)?;

    let node = Node::new(allocator, ptr);
    let s: Vec<u8> = node_to_bytes(&node)?;
    Ok(PyBytes::new(py, &s))
}

/// This module is a python module implemented in Rust.
#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_run_program, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_from_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_to_bytes, m)?)?;

    m.add_function(wrap_pyfunction!(deserialize_and_run_program, m)?)?;
    m.add("STRICT_MODE", STRICT_MODE)?;

    //m.add_class::<PyNode>()?;
    // m.add_class::<NativeOpLookup>()?;

    //m.add_class::<PyIntNode>()?;
    m.add_class::<PyIntAllocator>()?;

    //m.add_function(wrap_pyfunction!(serialized_length, m)?)?;
    m.add_class::<PyNaNode>()?;

    //m.add_function(wrap_pyfunction!(raise_eval_error, m)?)?;

    Ok(())
}

use crate::py::op_fn::PyOperatorHandler;
use crate::reduction::{EvalErr, Reduction};

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn py_run_program(
    py: Python,
    program: &PyCell<PyNaNode>,
    args: &PyCell<PyNaNode>,
    quote_kw: u8,
    apply_kw: u8,
    max_cost: Cost,
    opcode_lookup_by_name: HashMap<String, Vec<u8>>,
    py_callback: PyObject,
) -> PyResult<(Cost, PyObject)> {
    let allocator: &mut IntAllocator = &mut IntAllocator::new();

    let cache = new_mapping(py)?;
    let op_lookup = Box::new(PyOperatorHandler::new(
        opcode_lookup_by_name,
        py_callback,
        cache.clone(),
    )?);
    let program = native_for_py(py, &cache, program, allocator)?;
    let args = native_for_py(py, &cache, args, allocator)?;

    let r: Result<Reduction<i32>, EvalErr<i32>> = crate::run_program::run_program(
        allocator, &program, &args, quote_kw, apply_kw, max_cost, op_lookup, None,
    );

    match r {
        Ok(reduction) => {
            let r = py_for_native(py, &cache, &reduction.1, allocator)?;
            Ok((reduction.0, r.to_object(py)))
        }
        Err(eval_err) => {
            let node: PyObject = py_for_native(py, &cache, &eval_err.0, allocator)?.to_object(py);
            let s: String = eval_err.1;
            let s1: &str = &s;
            let msg: &PyString = PyString::new(py, s1);
            match raise_eval_error(py, &msg, node) {
                Err(x) => Err(x),
                _ => panic!(),
            }
        }
    }
}
