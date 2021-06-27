use pyo3::prelude::*;
use pyo3::wrap_pyfunction;

use super::lazy_node::LazyNode;
use super::run_generator::{
    PyConditionWithArgs, PySpendConditionSummary, __pyo3_get_function_run_generator,
};
use super::run_program::{
    __pyo3_get_function_deserialize_and_run_program,
    __pyo3_get_function_deserialize_and_run_program2,
    __pyo3_get_function_serialize_and_run_program, __pyo3_get_function_serialized_length,
};
use crate::run_program::STRICT_MODE;

/// This module is a python module implemented in Rust.
#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(serialize_and_run_program, m)?)?;
    m.add_function(wrap_pyfunction!(deserialize_and_run_program, m)?)?;
    m.add_function(wrap_pyfunction!(deserialize_and_run_program2, m)?)?;
    m.add_function(wrap_pyfunction!(run_generator, m)?)?;
    m.add("STRICT_MODE", STRICT_MODE)?;
    m.add_class::<LazyNode>()?;
    m.add_class::<PySpendConditionSummary>()?;
    m.add_class::<PyConditionWithArgs>()?;

    m.add_function(wrap_pyfunction!(serialized_length, m)?)?;

    Ok(())
}
