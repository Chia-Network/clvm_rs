use std::collections::HashMap;

use wasm_bindgen::prelude::*;

use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::err_utils::err;
use crate::f_table::{f_lookup_for_hashmap, FLookup};
use crate::more_ops::op_unknown;
use crate::node::Node;
use crate::reduction::Response;
use crate::run_program::{run_program, OperatorHandler};
use crate::serialize::{node_from_bytes, node_to_bytes};

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

struct OperatorHandlerWithMode {
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

#[wasm_bindgen]
pub fn run_clvm(program: &[u8], args: &[u8]) -> Vec<u8> {
    let quote_kw: u8 = 1;
    let apply_kw: u8 = 2;
    let max_cost: Cost = 1_000_000_000_000_000;

    let mut opcode_lookup_by_name = HashMap::<String, Vec<u8>>::new();
    for (v, s) in [
        (3, "op_if"),
        (4, "op_cons"),
        (5, "op_first"),
        (6, "op_rest"),
        (7, "op_listp"),
        (8, "op_raise"),
        (9, "op_eq"),
        (10, "op_gr_bytes"),
        (11, "op_sha256"),
        (12, "op_substr"),
        (13, "op_strlen"),
        (14, "op_concat"),
        (16, "op_add"),
        (17, "op_subtract"),
        (18, "op_multiply"),
        (19, "op_div"),
        (20, "op_divmod"),
        (21, "op_gr"),
        (22, "op_ash"),
        (23, "op_lsh"),
        (24, "op_logand"),
        (25, "op_logior"),
        (26, "op_logxor"),
        (27, "op_lognot"),
        (29, "op_point_add"),
        (30, "op_pubkey_for_exp"),
        (32, "op_not"),
        (33, "op_any"),
        (34, "op_all"),
        (36, "op_softfork"),
    ]
    .iter()
    {
        let v: Vec<u8> = vec![*v as u8];
        opcode_lookup_by_name.insert(s.to_string(), v);
    }

    let f_lookup = f_lookup_for_hashmap(opcode_lookup_by_name);
    let strict: bool = false;
    let f = OperatorHandlerWithMode { f_lookup, strict };
    let mut allocator = Allocator::new();
    let program = node_from_bytes(&mut allocator, program).unwrap();
    let args = node_from_bytes(&mut allocator, args).unwrap();
    let r = run_program(
        &mut allocator,
        program,
        args,
        &[quote_kw],
        &[apply_kw],
        max_cost,
        &f,
        None,
    );
    match r {
        Ok(reduction) => node_to_bytes(&Node::new(&allocator, reduction.1)).unwrap(),
        Err(_eval_err) => format!("{:?}", _eval_err).into(),
    }
}
