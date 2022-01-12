use crate::allocator::{Allocator, NodePtr};
use crate::core_ops::{op_cons, op_eq, op_first, op_if, op_listp, op_raise, op_rest};
use crate::cost::Cost;
use crate::dialect::Dialect;
use crate::err_utils::err;
use crate::more_ops::{
    op_add, op_all, op_any, op_ash, op_concat, op_div, op_div_deprecated, op_divmod, op_gr,
    op_gr_bytes, op_logand, op_logior, op_lognot, op_logxor, op_lsh, op_multiply, op_not,
    op_point_add, op_pubkey_for_exp, op_sha256, op_softfork, op_strlen, op_substr, op_subtract,
    op_unknown,
};
use crate::reduction::Response;

// division with negative numbers are disallowed
pub const NO_NEG_DIV: u32 = 0x0001;

// unknown operators are disallowed
// (otherwise they are no-ops with well defined cost)
pub const NO_UNKNOWN_OPS: u32 = 0x0002;

pub struct ChiaDialect {
    flags: u32,
}

impl ChiaDialect {
    pub fn new(flags: u32) -> ChiaDialect {
        ChiaDialect { flags }
    }
}

impl Dialect for ChiaDialect {
    fn op(
        &self,
        allocator: &mut Allocator,
        o: NodePtr,
        argument_list: NodePtr,
        max_cost: Cost,
    ) -> Response {
        let b = &allocator.atom(o);
        if b.len() != 1 {
            return if (self.flags & NO_UNKNOWN_OPS) != 0 {
                err(o, "unimplemented operator")
            } else {
                op_unknown(allocator, o, argument_list, max_cost)
            };
        }
        let f = match b[0] {
            3 => op_if,
            4 => op_cons,
            5 => op_first,
            6 => op_rest,
            7 => op_listp,
            8 => op_raise,
            9 => op_eq,
            10 => op_gr_bytes,
            11 => op_sha256,
            12 => op_substr,
            13 => op_strlen,
            14 => op_concat,
            // 15 ---
            16 => op_add,
            17 => op_subtract,
            18 => op_multiply,
            19 => {
                if (self.flags & NO_NEG_DIV) != 0 {
                    op_div_deprecated
                } else {
                    op_div
                }
            }
            20 => op_divmod,
            21 => op_gr,
            22 => op_ash,
            23 => op_lsh,
            24 => op_logand,
            25 => op_logior,
            26 => op_logxor,
            27 => op_lognot,
            // 28 ---
            29 => op_point_add,
            30 => op_pubkey_for_exp,
            // 31 ---
            32 => op_not,
            33 => op_any,
            34 => op_all,
            // 35 ---
            36 => op_softfork,
            _ => {
                if (self.flags & NO_UNKNOWN_OPS) != 0 {
                    return err(o, "unimplemented operator");
                } else {
                    return op_unknown(allocator, o, argument_list, max_cost);
                }
            }
        };
        f(allocator, argument_list, max_cost)
    }

    fn quote_kw(&self) -> &[u8] {
        &[1]
    }

    fn apply_kw(&self) -> &[u8] {
        &[2]
    }
}
