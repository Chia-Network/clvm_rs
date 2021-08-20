use crate::allocator::{Allocator, NodePtr};
use crate::core_ops::{op_cons, op_eq, op_first, op_if, op_listp, op_raise, op_rest};
use crate::cost::Cost;
use crate::err_utils::err;
use crate::more_ops::{
    op_add, op_all, op_any, op_ash, op_concat, op_div, op_divmod, op_gr, op_gr_bytes, op_logand,
    op_logior, op_lognot, op_logxor, op_lsh, op_multiply, op_not, op_point_add, op_pubkey_for_exp,
    op_sha256, op_softfork, op_strlen, op_substr, op_subtract, op_unknown,
};
use crate::reduction::Response;
use crate::run_program::{run_program, OperatorHandler, STRICT_MODE};

struct OpCallback {
    strict: bool,
}

impl OperatorHandler for OpCallback {
    fn op(
        &self,
        allocator: &mut Allocator,
        o: NodePtr,
        argument_list: NodePtr,
        max_cost: Cost,
    ) -> Response {
        let b = &allocator.atom(o);
        if b.len() != 1 {
            return if self.strict {
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
            19 => op_div,
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
                if self.strict {
                    return op_unknown(allocator, o, argument_list, max_cost);
                } else {
                    return err(o, "unimplemented operator");
                }
            }
        };
        f(allocator, argument_list, max_cost)
    }
}

pub fn run_chia_program(
    allocator: &mut Allocator,
    program: NodePtr,
    args: NodePtr,
    max_cost: Cost,
    flags: u32,
) -> Response {
    let f = OpCallback {
        strict: (flags & STRICT_MODE) != 0,
    };
    run_program(
        allocator,
        program,
        args,
        &[1], // quote
        &[2], // apply
        max_cost,
        &f,
        None,
    )
}
