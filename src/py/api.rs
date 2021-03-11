use std::collections::HashMap;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};
use pyo3::wrap_pyfunction;
use pyo3::PyObject;

use super::glue::{_serialize_from_bytes, _serialize_to_bytes};
//use super::int_allocator_gateway::{PyIntAllocator, PyIntNode};
use super::native_op_lookup::GenericNativeOpLookup;
use super::py_int_allocator::PyIntAllocator;
use super::py_na_node::{new_cache, PyNaNode};

//use super::run_program::{__pyo3_get_function_deserialize_and_run_program, STRICT_MODE};
use crate::int_allocator::IntAllocator;
use crate::py::f_table::{f_lookup_for_hashmap, FLookup};
//use crate::py::run_program::OperatorHandlerWithMode;
use crate::run_program::OperatorHandler;
use crate::serialize::{node_from_bytes, node_to_bytes};
use crate::{allocator, cost::Cost};

type AllocatorT<'a> = IntAllocator;
//type NodeClass = PyIntNode;

/*
#[pyclass]
pub struct NativeOpLookup {
    nol: usize, // Box<GenericNativeOpLookup<AllocatorT>>,
}

#[pymethods]
impl NativeOpLookup {
    #[new]
    fn new(opcode_lookup_by_name: HashMap<String, Vec<u8>>, unknown_op_callback: PyObject) -> Self {
        Self::new_from_gnol(Box::new(GenericNativeOpLookup::new(
            opcode_lookup_by_name,
            unknown_op_callback,
        )))
    }
}

impl Drop for NativeOpLookup {
    fn drop(&mut self) {
        let _b =
            unsafe { Box::from_raw(self.nol as *mut GenericNativeOpLookup<AllocatorT, NodeClass>) };
    }
}

impl NativeOpLookup {
    fn new_from_gnol(gnol: Box<GenericNativeOpLookup<AllocatorT, PyIntNode>>) -> Self {
        NativeOpLookup {
            nol: Box::into_raw(gnol) as usize,
        }
    }
}

impl NativeOpLookup {
    fn gnol<'a, 'p>(
        &self,
        _py: Python<'p>,
    ) -> &'a GenericNativeOpLookup<AllocatorT<'p>, PyIntNode> {
        unsafe { &*(self.nol as *const GenericNativeOpLookup<AllocatorT, NodeClass>) }
    }
}
*/

/*
#[pyfunction]
#[allow(clippy::too_many_arguments)]
fn py_run_program(
    py: Python,
    program: &PyCell<NodeClass>,
    args: &PyCell<NodeClass>,
    quote_kw: u8,
    apply_kw: u8,
    max_cost: Cost,
    opcode_lookup_by_name: HashMap<String, Vec<u8>>,
    py_callback: PyObject,
    //op_lookup: Py<NativeOpLookup>,
    //pre_eval: PyObject,
) -> PyResult<(Cost, PyObject)> {
    //let f_lookup = f_lookup_for_hashmap(opcode_lookup_by_name);
    //let strict: bool = false;
    //let f: Box<dyn OperatorHandler<IntAllocator>> =
    //   Box::new(OperatorHandlerWithMode { f_lookup, strict });

    _py_run_program(
        py,
        program,
        args,
        quote_kw,
        apply_kw,
        max_cost,
        opcode_lookup_by_name,
        py_callback,
    )
}
*/

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

const fn allocator_for_py(_py: Python) -> AllocatorT {
    AllocatorT::new()
}

#[pyfunction]
fn serialize_from_bytes<'p>(py: Python<'p>, blob: &[u8]) -> PyResult<&'p PyCell<PyNaNode>> {
    let mut py_int_allocator = PyCell::new(py, PyIntAllocator::default())?;
    let allocator: &mut IntAllocator = &mut py_int_allocator.borrow_mut().arena;
    let ptr = node_from_bytes(allocator, blob)?;
    PyNaNode::from_ptr(py, &py_int_allocator.to_object(py), ptr)
}

/*
#[pyfunction]
fn serialize_to_bytes(py: Python, sexp: &mut PyNaNode) -> PyResult<PyObject> {
    sexp.clear_native_view();
    let mut py_int_allocator: PyIntAllocator = sexp.arena.export(py)?;
    let allocator: &mut IntAllocator = &mut py_int_allocator.borrow_mut().arena;

    Ok(node_to_bytes(sexp))
}
*/

/// This module is a python module implemented in Rust.
#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_run_program, m)?)?;
    m.add_function(wrap_pyfunction!(serialize_from_bytes, m)?)?;
    //m.add_function(wrap_pyfunction!(serialize_to_bytes, m)?)?;

    //m.add_function(wrap_pyfunction!(deserialize_and_run_program, m)?)?;
    //m.add("STRICT_MODE", STRICT_MODE)?;

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
    let cache = new_cache(py)?;
    let arena = PyCell::new(
        py,
        PyIntAllocator {
            arena: IntAllocator::new(),
        },
    )?;
    let mut arena_ref = arena.borrow_mut();
    let mut allocator: &mut IntAllocator = &mut arena_ref.arena;

    let arena_as_obj = arena.to_object(py);
    println!("1");
    let program = PyNaNode::ptr(program, py, &cache, &arena_as_obj, allocator)?;
    println!("2");
    let args = PyNaNode::ptr(args, py, &cache, &arena_as_obj, allocator)?;
    println!("3");

    let op_lookup = Box::new(PyOperatorHandler::new(
        opcode_lookup_by_name,
        arena.to_object(py),
        py_callback,
    ));

    let r: Result<Reduction<i32>, EvalErr<i32>> = crate::run_program::run_program(
        allocator, &program, &args, quote_kw, apply_kw, max_cost, op_lookup, None,
    );

    match r {
        Ok(reduction) => {
            let r = PyNaNode::from_ptr(py, &arena.to_object(py), reduction.1)?;
            Ok((reduction.0, r.to_object(py)))
        }
        Err(eval_err) => {
            let node: PyObject = eval_err.0.to_object(py);
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
