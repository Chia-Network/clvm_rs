use std::cell::RefMut;
use std::collections::HashMap;
use std::rc::Rc;

use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::err_utils::err;
use crate::more_ops::op_unknown;
use crate::node::Node;
use crate::reduction::Response;
use crate::run_program::{run_program, OperatorHandler};
use crate::serialize::{node_from_bytes, node_to_bytes, serialized_length_from_bytes};

use super::arena::Arena;
use super::f_table::{f_lookup_for_hashmap, FLookup};
use super::lazy_node::LazyNode;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

pub const STRICT_MODE: u32 = 1;

pub struct OperatorHandlerWithMode {
    f_lookup: FLookup,
    strict: bool,
}

impl OperatorHandler for OperatorHandlerWithMode {
    fn op(
        &self,
        allocator: &mut Allocator,
        o: NodePtr,
        argument_list: NodePtr,
        max_cost: Cost,
    ) -> Response {
        let b = &allocator.atom(o);
        if b.len() == 1 {
            if let Some(f) = self.f_lookup[b[0] as usize] {
                return f(allocator, argument_list, max_cost);
            }
        }
        if self.strict {
            err(o, "unimplemented operator")
        } else {
            op_unknown(allocator, o, argument_list, max_cost)
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[pyfunction]
pub fn deserialize_and_run_program(
    py: Python,
    program: &[u8],
    args: &[u8],
    quote_kw: u8,
    apply_kw: u8,
    opcode_lookup_by_name: HashMap<String, Vec<u8>>,
    max_cost: Cost,
    flags: u32,
) -> PyResult<(Cost, PyObject)> {
    let arena = Arena::new_cell(py)?;
    let arena_borrowed = arena.borrow();
    let mut allocator_refcell: RefMut<Allocator> = arena_borrowed.allocator();
    let allocator: &mut Allocator = &mut allocator_refcell as &mut Allocator;
    let f_lookup = f_lookup_for_hashmap(opcode_lookup_by_name);
let strict: bool = (flags & STRICT_MODE) != 0;
    let f = OperatorHandlerWithMode { f_lookup, strict };
    let program = node_from_bytes(allocator, program)?;
    let args = node_from_bytes(allocator, args)?;

    let r = py.allow_threads(|| {
        run_program(
            allocator, program, args, &[quote_kw], &[apply_kw], max_cost, &f, None,
        )
    });
    match r {
        Ok(reduction) => Ok((
            reduction.0,
            arena_borrowed
                .as_python(py, allocator, reduction.1)?
                .to_object(py),
        )),
        Err(eval_err) => {
            let node_as_blob = node_to_bytes(&Node::new(&allocator, eval_err.0))?;
            let msg = eval_err.1;
            let ctx: &PyDict = PyDict::new(py);
            ctx.set_item("msg", msg)?;
            ctx.set_item("node_as_blob", node_as_blob)?;
            let r = py.run(
                "
from clvm import SExp
from clvm.EvalError import EvalError
from clvm.serialize import sexp_from_stream
import io
sexp = sexp_from_stream(io.BytesIO(bytes(node_as_blob)), SExp.to)
raise EvalError(msg, sexp)",
                None,
                Some(ctx),
            );
            match r {
                Err(x) => Err(x),
                Ok(_) => Ok((0, PyBytes::new(py, &[]).into())),
            }
        }
    }
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
    let f_lookup = f_lookup_for_hashmap(opcode_lookup_by_name);
    let strict: bool = (flags & STRICT_MODE) != 0;
    let f = OperatorHandlerWithMode { f_lookup, strict };
    let program = node_from_bytes(&mut allocator, program)?;
    let args = node_from_bytes(&mut allocator, args)?;

    let r = py.allow_threads(|| {
        run_program(
            &mut allocator,
            program,
            args,
            &[quote_kw],
            &[apply_kw],
            max_cost,
            &f,
            None,
        )
    });
    match r {
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
