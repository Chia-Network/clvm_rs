use std::collections::HashMap;

use crate::allocator::Allocator;
use crate::chia_dialect::OperatorHandlerWithMode;
use crate::cost::Cost;
use crate::dialect::Dialect;
use crate::py::adapt_response::adapt_response_to_py;
use crate::py::lazy_node::LazyNode;
use crate::reduction::Response;
use crate::run_program::STRICT_MODE;
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
    let strict: bool = (flags & STRICT_MODE) != 0;
    let program = node_from_bytes(allocator, program)?;
    let args = node_from_bytes(allocator, args)?;
    let dialect = Dialect::new(
        quote_kw,
        apply_kw,
        OperatorHandlerWithMode::new_with_hashmap(opcode_lookup_by_name, strict),
    );

    Ok(py.allow_threads(|| dialect.run_program(allocator, program, args, max_cost)))
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
