use std::collections::HashMap;

use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::dialect::Dialect;
use crate::err_utils::err;
use crate::f_table::{f_lookup_for_hashmap, FLookup};
use crate::more_ops::op_unknown;
use crate::operator_handler::OperatorHandler;
use crate::reduction::Response;

const QUOTE_KW: [u8; 1] = [1];
const APPLY_KW: [u8; 1] = [2];

pub struct OperatorHandlerWithMode {
    f_lookup: FLookup,
    strict: bool,
}

impl OperatorHandlerWithMode {
    pub fn new_with_hashmap(hashmap: HashMap<String, Vec<u8>>, strict: bool) -> Self {
        let f_lookup: FLookup = f_lookup_for_hashmap(hashmap);
        OperatorHandlerWithMode { f_lookup, strict }
    }
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

pub fn chia_opcode_mapping(deprecate_op_div: bool) -> HashMap<String, Vec<u8>> {
    let mut h = HashMap::new();
    let items = [
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
        (
            19,
            if deprecate_op_div {
                "op_div_deprecated"
            } else {
                "op_div"
            },
        ),
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
    ];
    for (k, v) in items {
        h.insert(v.to_string(), [k as u8].into());
    }
    h
}

pub fn chia_op_handler(strict: bool) -> OperatorHandlerWithMode {
    OperatorHandlerWithMode::new_with_hashmap(chia_opcode_mapping(strict), strict)
}

pub fn chia_dialect_with_handler<Handler: OperatorHandler>(handler: Handler) -> Dialect<Handler> {
    Dialect::new(&QUOTE_KW, &APPLY_KW, handler)
}

pub fn chia_dialect(strict: bool) -> Dialect<OperatorHandlerWithMode> {
    chia_dialect_with_handler(chia_op_handler(strict))
}
