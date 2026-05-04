#![allow(clippy::useless_conversion)]
use std::collections::HashMap;
use std::io;
use std::rc::Rc;

use super::lazy_node::LazyNode;
use crate::adapt_response::adapt_response;
use clvmr::allocator::{Allocator, NodePtr};
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
    SERDE_2026_MAGIC_PREFIX, deserialize_2026, node_to_bytes_serde_2026_level,
};

/// Sane "don't OOM the parser" default. clvm_rs has no consensus opinion;
/// downstream wrappers (e.g. chia_rs) supply their own caps.
const PY_DEFAULT_MAX_ATOM_LEN: usize = 1 << 20;
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

/// Deserialize a serde_2026 blob.  The input must start with
/// `SERDE_2026_MAGIC_PREFIX` (the same prefix `ser_2026` emits); the prefix is
/// stripped before calling the underlying decoder.  This makes `ser_2026` and
/// `deser_2026` a symmetric pair.
#[pyfunction]
#[pyo3(signature = (blob, *, max_atom_len=PY_DEFAULT_MAX_ATOM_LEN, strict=false))]
fn deser_2026(blob: &[u8], max_atom_len: usize, strict: bool) -> PyResult<LazyNode> {
    let body = blob
        .strip_prefix(SERDE_2026_MAGIC_PREFIX.as_slice())
        .ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(
                "deser_2026: blob is missing the serde_2026 magic prefix",
            )
        })?;
    let mut a = Allocator::new();
    let node = deserialize_2026(&mut a, body, max_atom_len, strict).map_err(eval_to_py)?;
    Ok(LazyNode::new(Rc::new(a), node))
}

/// Deserialize CLVM bytes, auto-detecting the format (classic, backrefs, or
/// serde_2026).  If the blob starts with the magic prefix
/// `fd ff 32 30 32 36`, it is treated as serde_2026; otherwise the backrefs
/// deserializer is used (which also handles plain classic format).
///
/// This is a Python convenience function — clvm_rs's Rust API doesn't have
/// an auto-switching counterpart. Consensus-aware callers should sniff the
/// prefix themselves and use their own caps.
#[pyfunction]
#[pyo3(signature = (blob, *, max_atom_len=PY_DEFAULT_MAX_ATOM_LEN, strict=false))]
fn deser_auto(blob: &[u8], max_atom_len: usize, strict: bool) -> PyResult<LazyNode> {
    let mut a = Allocator::new();
    let node = if let Some(body) = blob.strip_prefix(SERDE_2026_MAGIC_PREFIX.as_slice()) {
        deserialize_2026(&mut a, body, max_atom_len, strict).map_err(eval_to_py)?
    } else {
        node_from_bytes_backrefs(&mut a, blob).map_err(eval_to_py)?
    };
    Ok(LazyNode::new(Rc::new(a), node))
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

/// Serialize to serde_2026 format (always includes the magic prefix).
///
/// `level` selects the compression level. Levels above the highest implemented
/// level saturate to it, so passing `u32::MAX` always means "best available
/// compression". Currently only level 0 (left-first/fast) is implemented.
#[pyfunction]
#[pyo3(signature = (node, *, level=0))]
fn ser_2026(py: Python, node: &LazyNode, level: u32) -> PyResult<Py<PyBytes>> {
    let buf =
        node_to_bytes_serde_2026_level(node.allocator(), node.node(), level).map_err(eval_to_py)?;
    Ok(PyBytes::new(py, &buf).unbind())
}

/// Convert a Python CLVM tree (any object with `.atom` / `.pair` attributes)
/// into a `LazyNode` backed by a Rust `Allocator`, with full interning.
///
/// Uses three hash maps mirroring `intern_tree`:
/// 1. Python object identity (`id()`) -> NodePtr (prevents exponential blowup)
/// 2. Atom byte content -> NodePtr (deduplicates identical atoms)
/// 3. (left, right) pair -> NodePtr (deduplicates structurally identical pairs)
#[pyfunction]
fn clvm_tree_to_lazy_node(obj: Bound<'_, PyAny>) -> PyResult<LazyNode> {
    let mut allocator = Allocator::new();

    let mut identity_map: HashMap<usize, NodePtr> = HashMap::new();
    let mut atom_map: HashMap<Vec<u8>, NodePtr> = HashMap::new();
    let mut pair_map: HashMap<(NodePtr, NodePtr), NodePtr> = HashMap::new();

    enum WorkItem<'py> {
        Visit(Bound<'py, PyAny>),
        BuildPair {
            id: usize,
            left_id: usize,
            right_id: usize,
        },
    }

    let root_ptr = obj.as_ptr() as usize;
    let mut stack: Vec<WorkItem<'_>> = vec![WorkItem::Visit(obj)];

    while let Some(item) = stack.pop() {
        match item {
            WorkItem::Visit(pyobj) => {
                let id = pyobj.as_ptr() as usize;

                if identity_map.contains_key(&id) {
                    continue;
                }

                let atom_val: Option<Vec<u8>> = pyobj.getattr("atom")?.extract()?;

                if let Some(bytes) = atom_val {
                    let node = if let Some(&existing) = atom_map.get(&bytes) {
                        existing
                    } else {
                        let new_node = allocator
                            .new_atom(&bytes)
                            .map_err(|e| pyo3::exceptions::PyMemoryError::new_err(e.to_string()))?;
                        atom_map.insert(bytes, new_node);
                        new_node
                    };
                    identity_map.insert(id, node);
                } else {
                    let pair_val: Option<(Bound<'_, PyAny>, Bound<'_, PyAny>)> =
                        pyobj.getattr("pair")?.extract()?;

                    if let Some((left, right)) = pair_val {
                        let left_id = left.as_ptr() as usize;
                        let right_id = right.as_ptr() as usize;

                        let left_done = identity_map.contains_key(&left_id);
                        let right_done = identity_map.contains_key(&right_id);

                        if left_done && right_done {
                            let l = identity_map[&left_id];
                            let r = identity_map[&right_id];
                            let node = if let Some(&existing) = pair_map.get(&(l, r)) {
                                existing
                            } else {
                                let new_node = allocator.new_pair(l, r).map_err(|e| {
                                    pyo3::exceptions::PyMemoryError::new_err(e.to_string())
                                })?;
                                pair_map.insert((l, r), new_node);
                                new_node
                            };
                            identity_map.insert(id, node);
                        } else {
                            stack.push(WorkItem::BuildPair {
                                id,
                                left_id,
                                right_id,
                            });
                            if !right_done {
                                stack.push(WorkItem::Visit(right));
                            }
                            if !left_done {
                                stack.push(WorkItem::Visit(left));
                            }
                        }
                    } else {
                        return Err(pyo3::exceptions::PyValueError::new_err(
                            "CLVM object has neither .atom nor .pair",
                        ));
                    }
                }
            }
            WorkItem::BuildPair {
                id,
                left_id,
                right_id,
            } => {
                let l = identity_map[&left_id];
                let r = identity_map[&right_id];
                let node = if let Some(&existing) = pair_map.get(&(l, r)) {
                    existing
                } else {
                    let new_node = allocator
                        .new_pair(l, r)
                        .map_err(|e| pyo3::exceptions::PyMemoryError::new_err(e.to_string()))?;
                    pair_map.insert((l, r), new_node);
                    new_node
                };
                identity_map.insert(id, node);
            }
        }
    }

    let root = identity_map[&root_ptr];
    Ok(LazyNode::new(Rc::new(allocator), root))
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
    m.add_function(wrap_pyfunction!(ser_legacy, m)?)?;
    m.add_function(wrap_pyfunction!(ser_backrefs, m)?)?;
    m.add_function(wrap_pyfunction!(ser_2026, m)?)?;
    m.add_function(wrap_pyfunction!(clvm_tree_to_lazy_node, m)?)?;

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
