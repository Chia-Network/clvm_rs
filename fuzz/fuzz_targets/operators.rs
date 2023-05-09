#![no_main]
use libfuzzer_sys::fuzz_target;

use clvmr::allocator::{Allocator, NodePtr};
use clvmr::bls_ops::{
    op_bls_g1_multiply, op_bls_g1_negate, op_bls_g1_subtract, op_bls_g2_add, op_bls_g2_multiply,
    op_bls_g2_negate, op_bls_g2_subtract, op_bls_map_to_g1, op_bls_map_to_g2,
    op_bls_pairing_identity, op_bls_verify,
};
use clvmr::core_ops::{op_cons, op_eq, op_first, op_if, op_listp, op_raise, op_rest};
use clvmr::cost::Cost;
use clvmr::more_ops::{
    op_add, op_all, op_any, op_ash, op_coinid, op_concat, op_div, op_divmod, op_gr, op_gr_bytes,
    op_logand, op_logior, op_lognot, op_logxor, op_lsh, op_multiply, op_not, op_point_add,
    op_pubkey_for_exp, op_sha256, op_strlen, op_substr, op_subtract,
};
use clvmr::reduction::{EvalErr, Response};
use clvmr::serde::node_from_bytes;

type Opf = fn(&mut Allocator, NodePtr, Cost) -> Response;

const FUNS: [Opf; 41] = [
    op_if as Opf,
    op_cons as Opf,
    op_first as Opf,
    op_rest as Opf,
    op_listp as Opf,
    op_raise as Opf,
    op_eq as Opf,
    op_sha256 as Opf,
    op_add as Opf,
    op_subtract as Opf,
    op_multiply as Opf,
    op_div as Opf,
    op_divmod as Opf,
    op_substr as Opf,
    op_strlen as Opf,
    op_point_add as Opf,
    op_pubkey_for_exp as Opf,
    op_concat as Opf,
    op_gr as Opf,
    op_gr_bytes as Opf,
    op_logand as Opf,
    op_logior as Opf,
    op_logxor as Opf,
    op_lognot as Opf,
    op_ash as Opf,
    op_lsh as Opf,
    op_not as Opf,
    op_any as Opf,
    op_all as Opf,
    // the BLS extension
    op_coinid as Opf,
    op_bls_g1_subtract as Opf,
    op_bls_g1_multiply as Opf,
    op_bls_g1_negate as Opf,
    op_bls_g2_add as Opf,
    op_bls_g2_subtract as Opf,
    op_bls_g2_multiply as Opf,
    op_bls_g2_negate as Opf,
    op_bls_map_to_g1 as Opf,
    op_bls_map_to_g2 as Opf,
    op_bls_pairing_identity as Opf,
    op_bls_verify as Opf,
];

fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();

    let args = match node_from_bytes(&mut allocator, data) {
        Err(_) => {
            return;
        }
        Ok(r) => r,
    };

    let allocator_checkpoint = allocator.checkpoint();

    for op in FUNS {
        for max_cost in [11000000, 1100000, 110000, 10, 1, 0] {
            allocator.restore_checkpoint(&allocator_checkpoint);
            match op(&mut allocator, args, max_cost) {
                Err(EvalErr(n, msg)) => {
                    assert!(!msg.contains("internal error"));
                    // make sure n is a valid node in the allocator
                    allocator.sexp(n);
                }
                Ok(n) => {
                    // make sure n is a valid node in the allocator
                    allocator.sexp(n.1);
                    // TODO: it would be nice to be able to assert something
                    // like this, but not all operators check this very strictly
                    // (the main check is done by the interpreter). The main
                    // challenge is the malloc_cost(), which happens at the end,
                    // if the cost of allocating the return value is what makes
                    // is cross the max_cost limit, the operator still succeeds
                    // assert!(n.0 <= max_cost + 5000);
                }
            }
        }
    }
});
