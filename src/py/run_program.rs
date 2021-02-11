use crate::allocator::Allocator;
use crate::int_allocator::IntAllocator;
use crate::more_ops::op_unknown;
use crate::node::Node;
use crate::py::f_table::{make_f_lookup, FLookup};
use crate::reduction::{EvalErr, Reduction};
use crate::run_program::{run_program, OperatorHandler};
use crate::serialize::{node_from_bytes, node_to_bytes};
use lazy_static::lazy_static;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

lazy_static! {
    static ref F_TABLE: FLookup<IntAllocator> = make_f_lookup();
}

pub const STRICT_MODE: u32 = 1;

struct OperatorHandlerWithMode {
    strict: bool,
}

impl OperatorHandler<IntAllocator> for OperatorHandlerWithMode {
    fn op(
        &self,
        allocator: &IntAllocator,
        op: &[u8],
        argument_list: &u32,
    ) -> Result<Reduction<u32>, EvalErr<u32>> {
        if op.len() == 1 {
            if let Some(f) = F_TABLE[op[0] as usize] {
                let node_t: Node<IntAllocator> = Node::new(allocator, *argument_list);
                return f(&node_t);
            }
        }
        if self.strict {
            let op_arg = allocator.new_atom(op);
            allocator.err(&op_arg, "unimplemented operator")
        } else {
            op_unknown(op, &Node::new(allocator, *argument_list))
        }
    }
}

#[pyfunction]
pub fn serialize_and_run_program(
    py: Python,
    program: &[u8],
    args: &[u8],
    quote_kw: u8,
    apply_kw: u8,
    max_cost: u32,
    flags: u32,
) -> PyResult<(u32, Py<PyBytes>)> {
    let allocator = IntAllocator::new();
    let f: Box<dyn OperatorHandler<IntAllocator>> = Box::new(OperatorHandlerWithMode {
        strict: (flags & STRICT_MODE) != 0,
    });
    let program: u32 = node_from_bytes(&allocator, program).unwrap();

    let args: u32 = node_from_bytes(&allocator, args).unwrap();

    let r: Result<Reduction<u32>, EvalErr<u32>> = run_program(
        &allocator, &program, &args, quote_kw, apply_kw, max_cost, f, None,
    );
    match r {
        Ok(reduction) => {
            let node_as_blob = node_to_bytes(&Node::new(&allocator, reduction.1)).unwrap();
            let node_as_bytes: Py<PyBytes> = PyBytes::new(py, &node_as_blob).into();
            Ok((reduction.0, node_as_bytes))
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
                Ok(_) => Ok((0, PyBytes::new(py, &[]).into())),
            }
        }
    }
}
