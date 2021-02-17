use std::collections::HashMap;

use crate::allocator::Allocator;
use crate::err_utils::err;
use crate::int_allocator::IntAllocator;
use crate::more_ops::op_unknown;
use crate::node::Node;
use crate::py::f_table::{f_lookup_for_hashmap, FLookup};
use crate::reduction::Response;
use crate::run_program::{run_program, OperatorHandler};
use crate::serialize::{node_from_bytes, node_to_bytes};

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

pub const STRICT_MODE: u32 = 1;

struct OperatorHandlerWithMode<A: Allocator> {
    f_lookup: FLookup<A>,
    strict: bool,
}

impl<A: Allocator> OperatorHandler<A> for OperatorHandlerWithMode<A> {
    fn op(
        &self,
        allocator: &mut A,
        o: <A as Allocator>::AtomBuf,
        argument_list: &A::Ptr,
    ) -> Response<<A as Allocator>::Ptr> {
        let op = &allocator.buf(&o);
        if op.len() == 1 {
            if let Some(f) = self.f_lookup[op[0] as usize] {
                return f(allocator, argument_list.clone());
            }
        }
        if self.strict {
            let buf = op.to_vec();
            let op_arg = allocator.new_atom(&buf)?;
            err(op_arg, "unimplemented operator")
        } else {
            op_unknown(allocator, o, argument_list.clone())
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
    let mut opcode_lookup_by_name = HashMap::<String, Vec<u8>>::new();
    for (v, s) in [
        (4, "op_if"),
        (5, "op_cons"),
        (6, "op_first"),
        (7, "op_rest"),
        (8, "op_listp"),
        (9, "op_raise"),
        (10, "op_eq"),
        (11, "op_sha256"),
        (12, "op_add"),
        (13, "op_subtract"),
        (14, "op_multiply"),
        (15, "op_divmod"),
        (16, "op_substr"),
        (17, "op_strlen"),
        (18, "op_point_add"),
        (19, "op_pubkey_for_exp"),
        (20, "op_concat"),
        (22, "op_gr"),
        (23, "op_gr_bytes"),
        (24, "op_logand"),
        (25, "op_logior"),
        (26, "op_logxor"),
        (27, "op_lognot"),
        (28, "op_ash"),
        (29, "op_lsh"),
        (30, "op_not"),
        (31, "op_any"),
        (32, "op_all"),
        (33, "op_softfork"),
        (34, "op_div"),
    ]
    .iter()
    {
        let v: Vec<u8> = vec![*v as u8];
        opcode_lookup_by_name.insert(s.to_string(), v);
    }

    deserialize_and_run_program(
        py,
        program,
        args,
        quote_kw,
        apply_kw,
        opcode_lookup_by_name,
        max_cost,
        flags,
    )
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
    max_cost: u32,
    flags: u32,
) -> PyResult<(u32, Py<PyBytes>)> {
    let mut allocator = IntAllocator::new();
    let f_lookup = f_lookup_for_hashmap(opcode_lookup_by_name);
    let strict: bool = (flags & STRICT_MODE) != 0;
    let f: Box<dyn OperatorHandler<IntAllocator>> =
        Box::new(OperatorHandlerWithMode { f_lookup, strict });
    let program: i32 = node_from_bytes(&mut allocator, program).unwrap();
    let args: i32 = node_from_bytes(&mut allocator, args).unwrap();

    let r = run_program(
        &mut allocator,
        &program,
        &args,
        quote_kw,
        apply_kw,
        max_cost,
        f,
        None,
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
