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
use crate::keccak256_ops::op_keccak256;
use crate::more_ops::{
    op_add, op_all, op_any, op_ash, op_coinid, op_concat, op_div, op_divmod, op_gr, op_gr_bytes,
    op_logand, op_logior, op_lognot, op_logxor, op_lsh, op_mod, op_modpow, op_multiply, op_not,
    op_point_add, op_pubkey_for_exp, op_sha256, op_strlen, op_substr, op_subtract, op_unknown,
};
use crate::reduction::Response;
use crate::secp_ops::{op_secp256k1_verify, op_secp256r1_verify};

// unknown operators are disallowed
// (otherwise they are no-ops with well defined cost)
pub const NO_UNKNOWN_OPS: u32 = 0x0002;

// When set, limits the number of atom-bytes allowed to be allocated, as well as
// the number of pairs
pub const LIMIT_HEAP: u32 = 0x0004;

// enables the keccak256 op *outside* the softfork guard.
// This is a hard-fork and should only be enabled when it activates
pub const ENABLE_KECCAK_OPS_OUTSIDE_GUARD: u32 = 0x0100;

// The default mode when running grnerators in mempool-mode (i.e. the stricter
// mode)
pub const MEMPOOL_MODE: u32 = NO_UNKNOWN_OPS | LIMIT_HEAP;

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
        let flags = self.flags
            | match extension {
                // This is the default set of operators, so no special flags need to be added.
                OperatorSet::Default => 0,

                // Since BLS has been hardforked in universally, this has no effect.
                OperatorSet::Bls => 0,

                // Keccak is allowed as if it were a default operator, inside of the softfork guard.
                OperatorSet::Keccak => ENABLE_KECCAK_OPS_OUTSIDE_GUARD,
            };

        let op_len = allocator.atom_len(o);
        if op_len == 4 {
            // these are unknown operators with assigned cost
            // the formula is:
            // +---+---+---+------------+
            // | multiplier|XX | XXXXXX |
            // +---+---+---+---+--------+
            //  ^           ^    ^
            //  |           |    + 6 bits ignored when computing cost
            // cost         |
            // (3 bytes)    + 2 bits
            //                cost_function

            let b = allocator.atom(o);
            let opcode = u32::from_be_bytes(b.as_ref().try_into().unwrap());

            // the secp operators have a fixed cost of 1850000 and 1300000,
            // which makes the multiplier 0x1c3a8f and 0x0cf84f (there is an
            // implied +1) and cost function 0
            let f = match opcode {
                0x13d61f00 => op_secp256k1_verify,
                0x1c3a8f00 => op_secp256r1_verify,
                _ => {
                    return unknown_operator(allocator, o, argument_list, flags, max_cost);
                }
            };
            return f(allocator, argument_list, max_cost);
        }
        if op_len != 1 {
            return unknown_operator(allocator, o, argument_list, flags, max_cost);
        }
        let Some(op) = allocator.small_number(o) else {
            return unknown_operator(allocator, o, argument_list, flags, max_cost);
        };
        let f = match op {
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
            60 => op_modpow,
            61 => op_mod,
            62 if (flags & ENABLE_KECCAK_OPS_OUTSIDE_GUARD) != 0 => op_keccak256,
            _ => {
                return unknown_operator(allocator, o, argument_list, flags, max_cost);
            }
        };
        f(allocator, argument_list, max_cost)
    }

    fn quote_kw(&self) -> u32 {
        1
    }
    fn apply_kw(&self) -> u32 {
        2
    }
    fn softfork_kw(&self) -> u32 {
        36
    }

    // interpret the extension argument passed to the softfork operator, and
    // return the Operators it enables (or None) if we don't know what it means
    fn softfork_extension(&self, ext: u32) -> OperatorSet {
        match ext {
            // Extension 0 is for the BLS operators, and is still valid.
            // However, the extension doesn't add any addition opcodes,
            // because the BLS operators were hardforked into the main set.
            0 => OperatorSet::Bls,

            // Extension 1 is for the keccak256 operator.
            1 => OperatorSet::Keccak,

            // Extensions 2 and beyond are considered invalid by the mempool.
            // However, all future extensions are valid in consensus mode and reserved for future softforks.
            _ => OperatorSet::Default,
        }
    }

    fn allow_unknown_ops(&self) -> bool {
        (self.flags & NO_UNKNOWN_OPS) == 0
    }
}
