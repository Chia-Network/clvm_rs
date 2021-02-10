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

type AllocatorT<'a> = ArcAllocator;
type NodeClass = PyNode;

#[pyclass]
pub struct NativeOpLookup {
    nol: usize, // Box<GenericNativeOpLookup<AllocatorT>>,
}

#[pymethods]
impl NativeOpLookup {
    #[new]
    fn new(native_opcode_list: &[u8], unknown_op_callback: PyObject) -> Self {
        let native_lookup = make_f_lookup::<AllocatorT>();
        let mut f_lookup = [None; 256];
        for i in native_opcode_list.iter() {
            let idx = *i as usize;
            f_lookup[idx] = native_lookup[idx];
        }
        NativeOpLookup::new_from_gnol(Box::new(GenericNativeOpLookup::new(
            unknown_op_callback,
            f_lookup,
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
    fn new_from_gnol(gnol: Box<GenericNativeOpLookup<AllocatorT, PyNode>>) -> Self {
        NativeOpLookup {
            nol: Box::into_raw(gnol) as usize,
        }
    }
}

impl NativeOpLookup {
    fn gnol<'a, 'p>(&self, _py: Python<'p>) -> &'a GenericNativeOpLookup<AllocatorT<'p>, PyNode> {
        unsafe { &*(self.nol as *const GenericNativeOpLookup<AllocatorT, NodeClass>) }
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
    op_lookup: Py<NativeOpLookup>,
    pre_eval: PyObject,
) -> PyResult<(u32, NodeClass)> {
    let mut allocator = allocator_for_py(py);
    let op_lookup: &PyCell<NativeOpLookup> = op_lookup.as_ref(py);
    let op_lookup: PyRef<NativeOpLookup> = op_lookup.borrow();
    let op_lookup: Box<GenericNativeOpLookup<AllocatorT, NodeClass>> =
        Box::new(op_lookup.gnol(py).to_owned());
    _py_run_program(
        py,
        &mut allocator,
        program,
        args,
        quote_kw,
        apply_kw,
        max_cost,
        op_lookup,
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

fn allocator_for_py(_py: Python) -> AllocatorT {
    AllocatorT::new()
}

#[pyfunction]
fn serialize_from_bytes(py: Python, blob: &[u8]) -> NodeClass {
    let mut allocator = allocator_for_py(py);
    _serialize_from_bytes(&mut allocator, blob)
}

#[pyfunction]
fn serialize_to_bytes(py: Python, sexp: &PyAny) -> PyResult<PyObject> {
    let allocator = allocator_for_py(py);
    _serialize_to_bytes::<AllocatorT, NodeClass>(&allocator, py, sexp)
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
