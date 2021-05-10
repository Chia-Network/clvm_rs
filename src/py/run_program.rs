use std::cell::RefMut;
use std::collections::HashMap;

use crate::allocator::Allocator;
use crate::cost::Cost;
use crate::err_utils::err;
use crate::int_allocator::IntAllocator;
use crate::more_ops::op_unknown;
use crate::node::Node;
use crate::reduction::Response;
use crate::run_program::{run_program, OperatorHandler};
use crate::serialize::{node_from_bytes, node_to_bytes, serialized_length_from_bytes};

use super::f_table::{f_lookup_for_hashmap, FLookup};
use super::py_arena::PyArena;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

pub struct OperatorHandlerWithMode<A: Allocator> {
    f_lookup: FLookup<A>,
    strict: bool,
}

impl<A: Allocator> OperatorHandler<A> for OperatorHandlerWithMode<A> {
    fn op(
        &self,
        allocator: &mut A,
        o: <A as Allocator>::AtomBuf,
        argument_list: &A::Ptr,
        max_cost: Cost,
    ) -> Response<<A as Allocator>::Ptr> {
        let op = &allocator.buf(&o);
        if op.len() == 1 {
            if let Some(f) = self.f_lookup[op[0] as usize] {
                return f(allocator, argument_list.clone(), max_cost);
            }
        }
        if self.strict {
            let buf = op.to_vec();
            let op_arg = allocator.new_atom(&buf)?;
            err(op_arg, "unimplemented operator")
        } else {
            op_unknown(allocator, o, argument_list.clone(), max_cost)
        }
    }
}

#[allow(clippy::too_many_arguments)]
#[pyfunction]
pub fn deserialize_and_run_program(
    py: Python,
    program: &[u8],
    args: &[u8],
    quote_kw: &[u8],
    apply_kw: &[u8],
    opcode_lookup_by_name: HashMap<String, Vec<u8>>,
    max_cost: Cost,
    flags: u32,
) -> PyResult<(Cost, PyObject)> {
    let arena = PyArena::new(py)?;
    let mut allocator_refcell: RefMut<IntAllocator> = arena.allocator();
    let allocator: &mut IntAllocator = &mut allocator_refcell as &mut IntAllocator;
    let f_lookup = f_lookup_for_hashmap(opcode_lookup_by_name);
    let strict: bool = flags != 0;
    let f = OperatorHandlerWithMode { f_lookup, strict };
    let program = node_from_bytes(allocator, program)?;
    let args = node_from_bytes(allocator, args)?;

    let r = py.allow_threads(|| {
        run_program(
            allocator, &program, &args, quote_kw, apply_kw, max_cost, &f, None,
        )
    });
    match r {
        Ok(reduction) => Ok((
            reduction.0,
            arena
                .py_for_native(py, &reduction.1, allocator)?
                .to_object(py),
        )),
        Err(eval_err) => {
            let node_as_blob = node_to_bytes(&Node::new(allocator, eval_err.0))?;
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
