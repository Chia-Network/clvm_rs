use std::cell::RefMut;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use pyo3::wrap_pyfunction;
use pyo3::PyObject;

use crate::core_ops::*;
use crate::err_utils::u8_err;
use crate::int_allocator::IntAllocator;
use crate::more_ops::*;
use crate::node::Node;
use crate::serialize::node_to_bytes;

use super::arena::Arena;
use super::dialect::{Dialect, PyMultiOpFn};
use super::f_table::OpFn;
use super::native_op::NativeOp;
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
fn serialize_to_bytes<'p>(py: Python<'p>, sexp: &PyAny) -> PyResult<&'p PyBytes> {
    let arena_cell = Arena::new_cell(py)?;
    let arena = arena_cell.borrow();
    let mut allocator_refcell: RefMut<IntAllocator> = arena.allocator();
    let allocator: &mut IntAllocator = &mut allocator_refcell as &mut IntAllocator;

    let ptr = Arena::native_for_py(&arena, py, sexp, allocator)?;

    let node = Node::new(allocator, ptr);
    let s: Vec<u8> = node_to_bytes(&node)?;
    Ok(PyBytes::new(py, &s))
}

/// This module is a python module implemented in Rust.
#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Arena>()?;
    m.add_class::<Dialect>()?;

    m.add_function(wrap_pyfunction!(native_opcodes_dict, m)?)?;
    m.add_function(wrap_pyfunction!(serialized_length, m)?)?;

    m.add(
        "NATIVE_OP_UNKNOWN_STRICT",
        PyMultiOpFn::new(|_a, b, _op, _d| u8_err(_a, &b, "unimplemented operator")),
    )?;

    m.add("NATIVE_OP_UNKNOWN_NON_STRICT", PyMultiOpFn::new(op_unknown))?;

    m.add_function(wrap_pyfunction!(serialize_to_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(deserialize_and_run_program, m)?)?;

    Ok(())
}
