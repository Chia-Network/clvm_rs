use std::collections::HashMap;

use crate::allocator::Allocator;
use crate::chia_dialect::ChiaDialect;
use crate::cost::Cost;
use crate::py::adapt_response::adapt_response_to_py;
use crate::py::lazy_node::LazyNode;
use crate::py::runtime_dialect::RuntimeDialect;
use crate::reduction::Response;
use crate::run_program::{run_program, STRICT_MODE};
use crate::serialize::{node_from_bytes, serialized_length_from_bytes};

use pyo3::prelude::*;

#[allow(clippy::too_many_arguments)]
pub fn run_serialized_program(
    py: Python,
    allocator: &mut Allocator,
    quote_kw: &[u8],
    apply_kw: &[u8],
    opcode_lookup_by_name: HashMap<String, Vec<u8>>,
    program: &[u8],
    args: &[u8],
    max_cost: Cost,
    flags: u32,
) -> PyResult<Response> {
    let program = node_from_bytes(allocator, program)?;
    let args = node_from_bytes(allocator, args)?;
    let dialect = RuntimeDialect::new(
        opcode_lookup_by_name,
        quote_kw.to_vec(),
        apply_kw.to_vec(),
        (flags & STRICT_MODE) != 0,
    );

    Ok(py.allow_threads(|| run_program(allocator, &dialect, program, args, max_cost, None)))
}

#[pyfunction]
pub fn serialized_length(program: &[u8]) -> PyResult<u64> {
    Ok(serialized_length_from_bytes(program)?)
}

#[allow(clippy::too_many_arguments)]
#[pyfunction]
pub fn deserialize_and_run_program2(
    py: Python,
    program: &[u8],
    args: &[u8],
    quote_kw: u8,
    apply_kw: u8,
    opcode_lookup_by_name: HashMap<String, Vec<u8>>,
    max_cost: Cost,
    flags: u32,
) -> PyResult<(Cost, LazyNode)> {
    let mut allocator = Allocator::new();
    let r = run_serialized_program(
        py,
        &mut allocator,
        &[quote_kw],
        &[apply_kw],
        opcode_lookup_by_name,
        program,
        args,
        max_cost,
        flags,
    )?;
    adapt_response_to_py(py, allocator, r)
}

#[pyfunction]
pub fn run_chia_program(
    py: Python,
    program: &[u8],
    args: &[u8],
    max_cost: Cost,
    flags: u32,
) -> PyResult<(Cost, LazyNode)> {
    let mut allocator = Allocator::new();

    let r: Response = (|| -> PyResult<Response> {
        let program = node_from_bytes(&mut allocator, program)?;
        let args = node_from_bytes(&mut allocator, args)?;
        let dialect = ChiaDialect::new((flags & STRICT_MODE) != 0);

        Ok(py
            .allow_threads(|| run_program(&mut allocator, &dialect, program, args, max_cost, None)))
    })()?;
    adapt_response_to_py(py, allocator, r)
}
