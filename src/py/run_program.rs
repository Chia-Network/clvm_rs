use std::collections::HashMap;
use std::rc::Rc;

use crate::allocator::Allocator;
use crate::chia_dialect::OperatorHandlerWithMode;
use crate::cost::Cost;
use crate::dialect::Dialect;
use crate::py::lazy_node::LazyNode;
use crate::reduction::Response;
use crate::run_program::STRICT_MODE;
use crate::serialize::{node_from_bytes, serialized_length_from_bytes};

use pyo3::prelude::*;
use pyo3::types::PyDict;

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
    match run_serialized_program(
        py,
        &mut allocator,
        &[quote_kw],
        &[apply_kw],
        opcode_lookup_by_name,
        program,
        args,
        max_cost,
        flags,
    )? {
        Ok(reduction) => {
            let val = LazyNode::new(Rc::new(allocator), reduction.1);
            Ok((reduction.0, val))
        }
        Err(eval_err) => {
            let node = LazyNode::new(Rc::new(allocator), eval_err.0);
            let msg = eval_err.1;
            let ctx: &PyDict = PyDict::new(py);
            ctx.set_item("msg", msg)?;
            ctx.set_item("node", node)?;
            Err(py
                .run(
                    "
from clvm.EvalError import EvalError
raise EvalError(msg, node)",
                    None,
                    Some(ctx),
                )
                .unwrap_err())
        }
    }
}
