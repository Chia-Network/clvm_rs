use pyo3::prelude::*;
use pyo3::wrap_pyfunction;

use super::lazy_node::LazyNode;
use super::run_program::{
    __pyo3_get_function_deserialize_and_run_program2, __pyo3_get_function_run_chia_program,
    __pyo3_get_function_serialized_length,
};
use clvmr::chia_dialect::{NO_NEG_DIV, NO_UNKNOWN_OPS, LIMIT_HEAP};

pub const MEMPOOL_MODE: u32 = NO_NEG_DIV | NO_UNKNOWN_OPS | LIMIT_HEAP;

#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(deserialize_and_run_program2, m)?)?;
    m.add_function(wrap_pyfunction!(run_chia_program, m)?)?;
    m.add("NO_NEG_DIV", NO_NEG_DIV)?;
    m.add("NO_UNKNOWN_OPS", NO_UNKNOWN_OPS)?;
    m.add("LIMIT_HEAP", LIMIT_HEAP)?;
    m.add("MEMPOOL_MODE", MEMPOOL_MODE)?;
    m.add_class::<LazyNode>()?;

    m.add_function(wrap_pyfunction!(serialized_length, m)?)?;

    Ok(())
}
