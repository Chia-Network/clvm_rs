use crate::allocator::{Allocator, NodePtr};
use crate::bls_ops::{
    op_bls_g1_multiply, op_bls_g1_negate, op_bls_g1_subtract, op_bls_g2_add, op_bls_g2_multiply,
    op_bls_g2_negate, op_bls_g2_subtract, op_bls_map_to_g1, op_bls_map_to_g2,
    op_bls_pairing_identity, op_bls_verify,
};
use crate::core_ops::{op_cons, op_eq, op_first, op_if, op_listp, op_raise, op_rest};
use crate::cost::Cost;
use crate::dialect::{Dialect, OperatorSet};
use crate::err_utils::err;
use crate::more_ops::{
    op_add, op_all, op_any, op_ash, op_coinid, op_concat, op_div, op_divmod, op_gr, op_gr_bytes,
    op_logand, op_logior, op_lognot, op_logxor, op_lsh, op_multiply, op_not, op_point_add,
    op_pubkey_for_exp, op_sha256, op_strlen, op_substr, op_subtract, op_unknown,
};
use crate::reduction::Response;

// unknown operators are disallowed
// (otherwise they are no-ops with well defined cost)
pub const NO_UNKNOWN_OPS: u32 = 0x0002;

// When set, limits the number of atom-bytes allowed to be allocated, as well as
// the number of pairs
pub const LIMIT_HEAP: u32 = 0x0004;

// When set, enforce a stack size limit for CLVM programs
pub const LIMIT_STACK: u32 = 0x0008;

// When set, we allow softfork with extension 0 (which includes coinid and the
// BLS operators). This remains disabled until the soft-fork activates
pub const ENABLE_BLS_OPS: u32 = 0x0010;

// enables the BLS ops extensions *outside* the softfork guard. This is a
// hard-fork and should only be enabled when it activates
pub const ENABLE_BLS_OPS_OUTSIDE_GUARD: u32 = 0x0020;

// The default mode when running grnerators in mempool-mode (i.e. the stricter
// mode)
pub const MEMPOOL_MODE: u32 = NO_UNKNOWN_OPS | LIMIT_HEAP | LIMIT_STACK;

fn unknown_operator(
    allocator: &mut Allocator,
    o: NodePtr,
    args: NodePtr,
    flags: u32,
    max_cost: Cost,
) -> Response {
    if (flags & NO_UNKNOWN_OPS) != 0 {
        err(o, "unimplemented operator")
    } else {
        op_unknown(allocator, o, args, max_cost)
    }
}

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
        extension: OperatorSet,
    ) -> Response {
        let b = &allocator.atom(o);
        if b.len() != 1 {
            return unknown_operator(allocator, o, argument_list, self.flags, max_cost);
        }
        let f = match b[0] {
            // 1 = quote
            // 2 = apply
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
            // 36 = softfork
            _ => {
                if extension == OperatorSet::BLS || (self.flags & ENABLE_BLS_OPS_OUTSIDE_GUARD) != 0
                {
                    match b[0] {
                        48 => op_coinid,
                        49 => op_bls_g1_subtract,
                        50 => op_bls_g1_multiply,
                        51 => op_bls_g1_negate,
                        52 => op_bls_g2_add,
                        53 => op_bls_g2_subtract,
                        54 => op_bls_g2_multiply,
                        55 => op_bls_g2_negate,
                        56 => op_bls_map_to_g1,
                        57 => op_bls_map_to_g2,
                        58 => op_bls_pairing_identity,
                        59 => op_bls_verify,
                        _ => {
                            return unknown_operator(
                                allocator,
                                o,
                                argument_list,
                                self.flags,
                                max_cost,
                            );
                        }
                    }
                } else {
                    return unknown_operator(allocator, o, argument_list, self.flags, max_cost);
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

    fn softfork_kw(&self) -> &[u8] {
        &[36]
    }

    // interpret the extension argument passed to the softfork operator, and
    // return the Operators it enables (or None) if we don't know what it means
    // We have to pretend that we don't know about the BLS extensions until
    // after the soft-fork activation, which is controlled by the ENABLE_BLS_OPS
    // flag
    fn softfork_extension(&self, ext: u32) -> OperatorSet {
        match ext {
            0 => {
                if (self.flags & ENABLE_BLS_OPS) == 0 {
                    OperatorSet::Default
                } else {
                    OperatorSet::BLS
                }
            }
            // new extensions go here
            _ => OperatorSet::Default,
        }
    }

    fn stack_limit(&self) -> usize {
        if (self.flags & LIMIT_STACK) != 0 {
            20000000
        } else {
            usize::MAX
        }
    }

    fn allow_unknown_ops(&self) -> bool {
        (self.flags & NO_UNKNOWN_OPS) == 0
    }
}
