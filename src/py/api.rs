use std::cell::RefMut;
use std::collections::HashMap;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};
use pyo3::wrap_pyfunction;
use pyo3::PyObject;

use crate::core_ops::*;
use crate::cost::Cost;
use crate::err_utils::err;
use crate::int_allocator::IntAllocator;
use crate::more_ops::*;
use crate::node::Node;
use crate::reduction::{EvalErr, Reduction};
use crate::serialize::node_to_bytes;

use super::dialect::{Dialect, PyMultiOpFn};
use super::f_table::OpFn;
use super::native_op::NativeOp;
use super::op_fn::PyOperatorHandler;
use super::py_arena::PyArena;
use super::run_program::{
    __pyo3_get_function_deserialize_and_run_program, __pyo3_get_function_serialized_length,
};

#[pyfunction]
pub fn native_opcodes_dict(py: Python) -> PyResult<PyObject> {
    let opcode_lookup: [(OpFn<IntAllocator>, &str); 30] = [
        (op_if, "op_if"),
        (op_cons, "op_cons"),
        (op_first, "op_first"),
        (op_rest, "op_rest"),
        (op_listp, "op_listp"),
        (op_raise, "op_raise"),
        (op_eq, "op_eq"),
        (op_sha256, "op_sha256"),
        (op_add, "op_add"),
        (op_subtract, "op_subtract"),
        (op_multiply, "op_multiply"),
        (op_divmod, "op_divmod"),
        (op_substr, "op_substr"),
        (op_strlen, "op_strlen"),
        (op_point_add, "op_point_add"),
        (op_pubkey_for_exp, "op_pubkey_for_exp"),
        (op_concat, "op_concat"),
        (op_gr, "op_gr"),
        (op_gr_bytes, "op_gr_bytes"),
        (op_logand, "op_logand"),
        (op_logior, "op_logior"),
        (op_logxor, "op_logxor"),
        (op_lognot, "op_lognot"),
        (op_ash, "op_ash"),
        (op_lsh, "op_lsh"),
        (op_not, "op_not"),
        (op_any, "op_any"),
        (op_all, "op_all"),
        (op_softfork, "op_softfork"),
        (op_div, "op_div"),
    ];
    let r = PyDict::new(py);
    for (f, name) in opcode_lookup.iter() {
        r.set_item(name, PyCell::new(py, NativeOp::new(*f))?)?;
    }
    Ok(r.to_object(py))
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
fn serialize_to_bytes<'p>(py: Python<'p>, sexp: &PyAny) -> PyResult<&'p PyBytes> {
    let arena = PyArena::new_cell(py)?;
    let arena_borrowed = arena.borrow();
    let mut allocator_refcell: RefMut<IntAllocator> = arena_borrowed.allocator();
    let allocator: &mut IntAllocator = &mut allocator_refcell as &mut IntAllocator;

    let ptr = PyArena::native_for_py(arena, py, sexp, allocator)?;

    let node = Node::new(allocator, ptr);
    let s: Vec<u8> = node_to_bytes(&node)?;
    Ok(PyBytes::new(py, &s))
}

/// This module is a python module implemented in Rust.
#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Dialect>()?;
    m.add_class::<PyArena>()?;

    m.add_function(wrap_pyfunction!(native_opcodes_dict, m)?)?;

    m.add_function(wrap_pyfunction!(serialized_length, m)?)?;

    m.add(
        "NATIVE_OP_UNKNOWN_STRICT",
        PyMultiOpFn::new(|_a, _b, op, _d| err(op, "unimplemented operator")),
    )?;

    m.add("NATIVE_OP_UNKNOWN_NON_STRICT", PyMultiOpFn::new(op_unknown))?;

    m.add_function(wrap_pyfunction!(py_run_program, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_to_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(deserialize_and_run_program, m)?)?;

    Ok(())
}

#[pyfunction]
#[allow(clippy::too_many_arguments)]
pub fn py_run_program<'p>(
    py: Python<'p>,
    program: &PyAny,
    args: &PyAny,
    quote_kw: &[u8],
    apply_kw: &[u8],
    max_cost: Cost,
    opcode_lookup_by_name: HashMap<String, Vec<u8>>,
    py_callback: PyObject,
) -> PyResult<(Cost, PyObject)> {
    let arena = PyArena::new_cell(py)?;
    let arena_borrowed = arena.borrow();
    let mut allocator_refcell: RefMut<IntAllocator> = arena_borrowed.allocator();
    let allocator: &mut IntAllocator = &mut allocator_refcell as &mut IntAllocator;

    let op_lookup = PyOperatorHandler::new(opcode_lookup_by_name, py_callback, arena)?;
    let program = PyArena::native_for_py(arena, py, program, allocator)?;
    let args = PyArena::native_for_py(arena, py, args, allocator)?;

    let r: Result<Reduction<i32>, EvalErr<i32>> = crate::run_program::run_program(
        allocator, &program, &args, quote_kw, apply_kw, max_cost, &op_lookup, None,
    );

    match r {
        Ok(reduction) => {
            let r = arena_borrowed.py_for_native(py, &reduction.1, allocator)?;
            Ok((reduction.0, r.to_object(py)))
        }
        Err(eval_err) => {
            let node: PyObject = arena_borrowed
                .py_for_native(py, &eval_err.0, allocator)?
                .to_object(py);
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
