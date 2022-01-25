use clvm_rs::py::api::clvm_rs_module;
use pyo3::prelude::*;

/// This module is a python module implemented in Rust.
#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    clvm_rs_module(_py, m)
}
