use crate::allocator::Allocator;
use crate::int_allocator::IntAllocator;
use crate::node::Node;
use crate::py::f_table::{make_f_lookup, FLookup};
use crate::reduction::{EvalErr, Reduction};
use crate::run_program::run_program;
use crate::serialize::{node_from_bytes, node_to_bytes};
use crate::types::OperatorHandler;
use lazy_static::lazy_static;
use pyo3::prelude::*;
use pyo3::types::PyDict;

lazy_static! {
    static ref F_TABLE: FLookup<u32> = make_f_lookup();
}

pub fn operator_handler2(
    allocator: &dyn Allocator<u32>,
    op: &[u8],
    argument_list: &u32,
) -> Result<Reduction<u32>, EvalErr<u32>> {
    if op.len() == 1 {
        if let Some(f) = F_TABLE[op[0] as usize] {
            let node_t: Node<u32> = Node::new(allocator, argument_list.clone());
            return f(&node_t);
        }
    }
    let op_arg = allocator.new_atom(op);
    allocator.err(&op_arg, "unimplemented operator")
}

#[pyfunction]
pub fn py_run_program2(
    py: Python,
    program: &[u8],
    args: &[u8],
    quote_kw: u8,
    apply_kw: u8,
    max_cost: u32,
) -> PyResult<(u32, Vec<u8>)> {
    let allocator = IntAllocator::new();
    let f: OperatorHandler<u32> = Box::new(operator_handler2);

    let program: u32 = node_from_bytes(&allocator, program).unwrap();

    let args: u32 = node_from_bytes(&allocator, args).unwrap();

    let r: Result<Reduction<u32>, EvalErr<u32>> = run_program(
        &allocator, &program, &args, quote_kw, apply_kw, max_cost, &f, None,
    );
    match r {
        Ok(reduction) => {
            let node_as_blob = node_to_bytes(&Node::new(&allocator, reduction.1)).unwrap();
            Ok((reduction.0, node_as_blob))
        }
        Err(eval_err) => {
            let node_as_blob = node_to_bytes(&Node::new(&allocator, eval_err.0)).unwrap();
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
                Ok(_) => Ok((0, vec![])),
            }
        }
    }
}
