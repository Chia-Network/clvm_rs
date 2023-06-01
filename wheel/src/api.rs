use std::io;

use super::lazy_node::LazyNode;
use crate::adapt_response::adapt_response;
use clvmr::allocator::Allocator;
use clvmr::chia_dialect::ChiaDialect;
use clvmr::cost::Cost;
use clvmr::reduction::Response;
use clvmr::run_program::run_program;
use clvmr::serde::{node_from_bytes, parse_triples, serialized_length_from_bytes, ParsedTriple};
use clvmr::{LIMIT_HEAP, MEMPOOL_MODE, NO_UNKNOWN_OPS};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};
use pyo3::wrap_pyfunction;

#[pyfunction]
pub fn serialized_length(program: &[u8]) -> PyResult<u64> {
    Ok(serialized_length_from_bytes(program)?)
}

#[pyfunction]
pub fn run_serialized_chia_program(
    py: Python,
    program: &[u8],
    args: &[u8],
    max_cost: Cost,
    flags: u32,
) -> PyResult<(u64, LazyNode)> {
    let mut allocator = if flags & LIMIT_HEAP != 0 {
        Allocator::new_limited(500000000, 62500000, 62500000)
    } else {
        Allocator::new()
    };

    let r: Response = (|| -> PyResult<Response> {
        let program = node_from_bytes(&mut allocator, program)?;
        let args = node_from_bytes(&mut allocator, args)?;
        let dialect = ChiaDialect::new(flags);

        Ok(py.allow_threads(|| run_program(&mut allocator, &dialect, program, args, max_cost)))
    })()?;
    adapt_response(py, allocator, r)
}

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
fn deserialize_as_tree(
    py: Python,
    blob: &[u8],
    calculate_tree_hashes: bool,
) -> PyResult<(Vec<PyObject>, Option<Vec<PyObject>>)> {
    let mut cursor = io::Cursor::new(blob);
    let (r, tree_hashes) = parse_triples(&mut cursor, calculate_tree_hashes)?;
    let r = r.iter().map(|pt| tuple_for_parsed_triple(py, pt)).collect();
    let s = tree_hashes.map(|ths| ths.iter().map(|b| PyBytes::new(py, b).into()).collect());
    Ok((r, s))
}

#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_serialized_chia_program, m)?)?;
    m.add_function(wrap_pyfunction!(serialized_length, m)?)?;
    m.add_function(wrap_pyfunction!(deserialize_as_tree, m)?)?;

    m.add("NO_UNKNOWN_OPS", NO_UNKNOWN_OPS)?;
    m.add("LIMIT_HEAP", LIMIT_HEAP)?;
    m.add("MEMPOOL_MODE", MEMPOOL_MODE)?;
    m.add_class::<LazyNode>()?;

    Ok(())
}
