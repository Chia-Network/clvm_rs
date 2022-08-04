use std::io;

use pyo3::prelude::*;
use pyo3::types::PyTuple;
use pyo3::wrap_pyfunction;

use super::lazy_node::LazyNode;
use super::run_program::{
    __pyo3_get_function_deserialize_and_run_program2, __pyo3_get_function_run_chia_program,
    __pyo3_get_function_serialized_length,
};
use clvmr::{LIMIT_HEAP, NO_NEG_DIV, NO_UNKNOWN_OPS, LIMIT_STACK, MEMPOOL_MODE};
use clvmr::chia_dialect::{NO_NEG_DIV, NO_UNKNOWN_OPS};
use clvmr::serialize::{parse_triples, ParsedTriple};

fn tuple_for_parsed_triple(py: Python<'_>, p: &ParsedTriple) -> PyObject {
    let tuple = match p {
        ParsedTriple::Atom {
            start,
            end,
            atom_offset,
        } => PyTuple::new(py, [*start, *end, *atom_offset as u64]),
        ParsedTriple::Pair {
            start,
            end,
            right_index,
        } => PyTuple::new(py, [*start, *end, *right_index as u64]),
    };
    tuple.into_py(py)
}

#[pyfunction]
fn deserialize_as_triples(py: Python, blob: &[u8]) -> PyResult<Vec<PyObject>> {
    let mut cursor = io::Cursor::new(blob);
    let r = parse_triples(&mut cursor)?;
    let r = r.iter().map(|pt| tuple_for_parsed_triple(py, pt)).collect();
    Ok(r)
}

#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(deserialize_and_run_program2, m)?)?;
    m.add_function(wrap_pyfunction!(run_chia_program, m)?)?;
    m.add_function(wrap_pyfunction!(deserialize_as_triples, m)?)?;
    m.add("NO_NEG_DIV", NO_NEG_DIV)?;
    m.add("NO_UNKNOWN_OPS", NO_UNKNOWN_OPS)?;
    m.add("LIMIT_HEAP", LIMIT_HEAP)?;
    m.add("LIMIT_STACK", LIMIT_STACK)?;
    m.add("MEMPOOL_MODE", MEMPOOL_MODE)?;
    m.add_class::<LazyNode>()?;

    m.add_function(wrap_pyfunction!(serialized_length, m)?)?;

    Ok(())
}
