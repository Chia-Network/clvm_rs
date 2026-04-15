#![allow(clippy::useless_conversion)]
use std::io;
use std::rc::Rc;

use super::lazy_node::LazyNode;
use crate::adapt_response::adapt_response;
use clvmr::allocator::Allocator;
use clvmr::chia_dialect::ChiaDialect;
use clvmr::chia_dialect::{ClvmFlags, MEMPOOL_MODE};
use clvmr::cost::Cost;
use clvmr::error::EvalErr;
use clvmr::reduction::Response;
use clvmr::run_program::run_program;
use clvmr::serde::{
    ParsedTriple, node_from_bytes, node_from_bytes_backrefs, node_to_bytes, node_to_bytes_backrefs,
    parse_triples, serialized_length_from_bytes,
};
use clvmr::serde_2026::{
    DeserializeLimits, deserialize_2026, node_from_bytes_auto, node_to_bytes_serde_2026,
    serialize_2026,
};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

fn eval_to_py(err: EvalErr) -> PyErr {
    // Rarely Used in python bindings.
    pyo3::exceptions::PyValueError::new_err(err.to_string())
}

#[pyfunction]
pub fn serialized_length(program: &[u8]) -> PyResult<u64> {
    serialized_length_from_bytes(program).map_err(eval_to_py)
}

#[pyfunction]
pub fn run_serialized_chia_program(
    py: Python,
    program: &[u8],
    args: &[u8],
    max_cost: Cost,
    flags: u32,
) -> PyResult<(u64, LazyNode)> {
    let flags = ClvmFlags::from_bits_truncate(flags);
    let mut allocator = if flags.contains(ClvmFlags::LIMIT_HEAP) {
        Allocator::new_limited(500000000)
    } else {
        Allocator::new()
    };

    let r: Response = (|| -> PyResult<Response> {
        let program = node_from_bytes(&mut allocator, program).map_err(eval_to_py)?;
        let args = node_from_bytes(&mut allocator, args).map_err(eval_to_py)?;
        let dialect = ChiaDialect::new(flags);

        Ok(py.detach(|| run_program(&mut allocator, &dialect, program, args, max_cost)))
    })()?;
    adapt_response(py, allocator, r)
}

fn tuple_for_parsed_triple(py: Python<'_>, p: &ParsedTriple) -> PyResult<Py<PyAny>> {
    let tuple = match p {
        ParsedTriple::Atom {
            start,
            end,
            atom_offset,
        } => PyTuple::new(py, [*start, *end, *atom_offset as u64])?,
        ParsedTriple::Pair {
            start,
            end,
            right_index,
        } => PyTuple::new(py, [*start, *end, *right_index as u64])?,
    };
    Ok(tuple.unbind().into_any())
}

#[pyfunction]
#[allow(clippy::type_complexity)]
fn deserialize_as_tree(
    py: Python,
    blob: &[u8],
    calculate_tree_hashes: bool,
) -> PyResult<(Vec<Py<PyAny>>, Option<Vec<Py<PyAny>>>)> {
    let mut cursor = io::Cursor::new(blob);
    let (r, tree_hashes) = parse_triples(&mut cursor, calculate_tree_hashes).map_err(eval_to_py)?;
    let r = r
        .iter()
        .map(|pt| tuple_for_parsed_triple(py, pt))
        .collect::<PyResult<Vec<_>>>()?;
    let s = tree_hashes.map(|ths| {
        ths.iter()
            .map(|b| PyBytes::new(py, b).unbind().into_any())
            .collect()
    });
    Ok((r, s))
}

// --- Deserialize functions: bytes -> LazyNode ---

#[pyfunction]
fn deser_legacy(blob: &[u8]) -> PyResult<LazyNode> {
    let mut a = Allocator::new();
    let node = node_from_bytes(&mut a, blob).map_err(eval_to_py)?;
    Ok(LazyNode::new(Rc::new(a), node))
}

#[pyfunction]
fn deser_backrefs(blob: &[u8]) -> PyResult<LazyNode> {
    let mut a = Allocator::new();
    let node = node_from_bytes_backrefs(&mut a, blob).map_err(eval_to_py)?;
    Ok(LazyNode::new(Rc::new(a), node))
}

fn make_limits(max_atom_len: Option<usize>, max_input_bytes: Option<usize>) -> DeserializeLimits {
    let mut limits = DeserializeLimits::default();
    if let Some(v) = max_atom_len {
        limits.max_atom_len = v;
    }
    if let Some(v) = max_input_bytes {
        limits.max_input_bytes = v;
    }
    limits
}

#[pyfunction]
#[pyo3(signature = (blob, *, max_atom_len=None, max_input_bytes=None))]
fn deser_2026(
    blob: &[u8],
    max_atom_len: Option<usize>,
    max_input_bytes: Option<usize>,
) -> PyResult<LazyNode> {
    let mut a = Allocator::new();
    let limits = make_limits(max_atom_len, max_input_bytes);
    let node = deserialize_2026(&mut a, blob, limits).map_err(eval_to_py)?;
    Ok(LazyNode::new(Rc::new(a), node))
}

/// Deserialize CLVM bytes, auto-detecting the format (classic, backrefs, or
/// serde_2026).  If the blob starts with the magic prefix
/// `fd ff 32 30 32 36`, it is
/// treated as serde_2026; otherwise the backrefs deserializer is used (which
/// also handles plain classic format).
#[pyfunction]
#[pyo3(signature = (blob, *, max_atom_len=None, max_input_bytes=None))]
fn deser_auto(
    blob: &[u8],
    max_atom_len: Option<usize>,
    max_input_bytes: Option<usize>,
) -> PyResult<LazyNode> {
    let mut a = Allocator::new();
    let limits = make_limits(max_atom_len, max_input_bytes);
    let node = node_from_bytes_auto(&mut a, blob, limits).map_err(eval_to_py)?;
    Ok(LazyNode::new(Rc::new(a), node))
}

/// Intern a tree: deduplicate atoms and pairs, returning a new LazyNode
/// with an interned allocator. This validates that the tree is properly
/// interned (no duplicate nodes by content/structure).
#[pyfunction]
fn intern(node: &LazyNode) -> PyResult<LazyNode> {
    use clvmr::serde::intern_tree;

    let interned = intern_tree(node.allocator(), node.node()).map_err(eval_to_py)?;
    Ok(LazyNode::new(Rc::new(interned.allocator), interned.root))
}

// --- Serialize functions: LazyNode -> bytes ---

#[pyfunction]
fn ser_legacy(py: Python, node: &LazyNode) -> PyResult<Py<PyBytes>> {
    let bytes = node_to_bytes(node.allocator(), node.node()).map_err(eval_to_py)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

#[pyfunction]
fn ser_backrefs(py: Python, node: &LazyNode) -> PyResult<Py<PyBytes>> {
    let bytes = node_to_bytes_backrefs(node.allocator(), node.node()).map_err(eval_to_py)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

#[pyfunction]
fn ser_2026(py: Python, node: &LazyNode) -> PyResult<Py<PyBytes>> {
    let bytes = serialize_2026(node.allocator(), node.node()).map_err(eval_to_py)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

/// Serialize to serde_2026 format **with** the `fd ff 32 30 32 36` magic prefix.
/// Use `deser_auto` to deserialize the result.
#[pyfunction]
fn ser_2026_prefixed(py: Python, node: &LazyNode) -> PyResult<Py<PyBytes>> {
    let bytes = node_to_bytes_serde_2026(node.allocator(), node.node()).map_err(eval_to_py)?;
    Ok(PyBytes::new(py, &bytes).unbind())
}

#[pymodule]
fn clvm_rs(_py: Python, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(run_serialized_chia_program, m)?)?;
    m.add_function(wrap_pyfunction!(serialized_length, m)?)?;
    m.add_function(wrap_pyfunction!(deserialize_as_tree, m)?)?;
    m.add_function(wrap_pyfunction!(deser_legacy, m)?)?;
    m.add_function(wrap_pyfunction!(deser_backrefs, m)?)?;
    m.add_function(wrap_pyfunction!(deser_2026, m)?)?;
    m.add_function(wrap_pyfunction!(deser_auto, m)?)?;
    m.add_function(wrap_pyfunction!(intern, m)?)?;
    m.add_function(wrap_pyfunction!(ser_legacy, m)?)?;
    m.add_function(wrap_pyfunction!(ser_backrefs, m)?)?;
    m.add_function(wrap_pyfunction!(ser_2026, m)?)?;
    m.add_function(wrap_pyfunction!(ser_2026_prefixed, m)?)?;

    m.add("NO_UNKNOWN_OPS", ClvmFlags::NO_UNKNOWN_OPS.bits())?;
    m.add("LIMIT_HEAP", ClvmFlags::LIMIT_HEAP.bits())?;
    m.add("MEMPOOL_MODE", MEMPOOL_MODE.bits())?;
    m.add("ENABLE_SHA256_TREE", ClvmFlags::ENABLE_SHA256_TREE.bits())?;
    m.add("ENABLE_SECP_OPS", ClvmFlags::ENABLE_SECP_OPS.bits())?;
    m.add("DISABLE_OP", ClvmFlags::DISABLE_OP.bits())?;
    m.add("CANONICAL_INTS", ClvmFlags::CANONICAL_INTS.bits())?;
    m.add_class::<LazyNode>()?;

    Ok(())
}
