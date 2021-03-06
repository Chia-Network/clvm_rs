use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use crate::allocator::Allocator;
use crate::cost::Cost;
use crate::err_utils::err;
use crate::int_allocator::IntAllocator;
use crate::more_ops::op_unknown;
use crate::node::Node;
use crate::py::f_table::{f_lookup_for_hashmap, FLookup};
use crate::py::int_allocator_gateway::{PyIntAllocator, PyIntNode};
use crate::reduction::Response;
use crate::run_program::{run_program, OperatorHandler};
use crate::serialize::{node_from_bytes, node_to_bytes, serialized_length_from_bytes};

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

pub const STRICT_MODE: u32 = 1;

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
    quote_kw: u8,
    apply_kw: u8,
    opcode_lookup_by_name: HashMap<String, Vec<u8>>,
    max_cost: Cost,
    flags: u32,
) -> PyResult<(Cost, PyObject)> {
    let mut allocator = IntAllocator::new();
    let f_lookup = f_lookup_for_hashmap(opcode_lookup_by_name);
    let strict: bool = (flags & STRICT_MODE) != 0;
    let f: Box<dyn OperatorHandler<IntAllocator> + Send> =
        Box::new(OperatorHandlerWithMode { f_lookup, strict });
    let program = node_from_bytes(&mut allocator, program)?;
    let args = node_from_bytes(&mut allocator, args)?;

    let r = py.allow_threads(|| {
        run_program(
            &mut allocator,
            &program,
            &args,
            quote_kw,
            apply_kw,
            max_cost,
            f,
            None,
        )
    });
    match r {
        Ok(reduction) => {
            let py_int_allocator = PyCell::new(py, PyIntAllocator { arena: allocator })?;
            let py_int_node = PyCell::new(
                py,
                PyIntNode {
                    arena: py_int_allocator.to_object(py),
                    py_view: RefCell::new(None),
                    native_view: Cell::new(Some(reduction.1)),
                },
            )?;
            Ok((reduction.0, py_int_node.to_object(py)))
        }
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
