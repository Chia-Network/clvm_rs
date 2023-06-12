use std::collections::HashMap;

use crate::allocator::{Allocator, NodePtr};
use crate::bls_ops::{
    op_bls_g1_multiply, op_bls_g1_negate, op_bls_g1_subtract, op_bls_g2_add, op_bls_g2_multiply,
    op_bls_g2_negate, op_bls_g2_subtract, op_bls_map_to_g1, op_bls_map_to_g2,
    op_bls_pairing_identity, op_bls_verify,
};
use crate::core_ops::{op_cons, op_eq, op_first, op_if, op_listp, op_raise, op_rest};
use crate::cost::Cost;
use crate::more_ops::{
    op_add, op_all, op_any, op_ash, op_concat, op_div, op_divmod, op_gr, op_gr_bytes, op_logand,
    op_logior, op_lognot, op_logxor, op_lsh, op_mod, op_modpow, op_multiply, op_not, op_point_add,
    op_pubkey_for_exp, op_sha256, op_strlen, op_substr, op_subtract,
};
use crate::reduction::Response;
use crate::secp_ops::{op_secp256k1_verify, op_secp256r1_verify};

type OpFn = fn(&mut Allocator, NodePtr, Cost) -> Response;

pub type FLookup = [Option<OpFn>; 256];

pub fn opcode_by_name(name: &str) -> Option<OpFn> {
    let opcode_lookup: [(OpFn, &str); 44] = [
        (op_if, "op_if"),
        (op_cons, "op_cons"),
        (op_first, "op_first"),
        (op_rest, "op_rest"),
        (op_listp, "op_listp"),
        (op_raise, "op_raise"),
        (op_eq, "op_eq"),
        (op_sha256, "op_sha256"),
        (op_add, "op_add"),
        (op_subtract, "op_subtract"),
        (op_multiply, "op_multiply"),
        (op_modpow, "op_modpow"),
        (op_divmod, "op_divmod"),
        (op_mod, "op_mod"),
        (op_substr, "op_substr"),
        (op_strlen, "op_strlen"),
        (op_point_add, "op_point_add"),
        (op_pubkey_for_exp, "op_pubkey_for_exp"),
        (op_concat, "op_concat"),
        (op_gr, "op_gr"),
        (op_gr_bytes, "op_gr_bytes"),
        (op_logand, "op_logand"),
        (op_logior, "op_logior"),
        (op_logxor, "op_logxor"),
        (op_lognot, "op_lognot"),
        (op_ash, "op_ash"),
        (op_lsh, "op_lsh"),
        (op_not, "op_not"),
        (op_any, "op_any"),
        (op_all, "op_all"),
        (op_div, "op_div"),
        (op_bls_g1_subtract, "op_g1_subtract"),
        (op_bls_g1_multiply, "op_g1_multiply"),
        (op_bls_g1_negate, "op_g1_negate"),
        (op_bls_g2_add, "op_g2_add"),
        (op_bls_g2_subtract, "op_g2_subtract"),
        (op_bls_g2_multiply, "op_g2_multiply"),
        (op_bls_g2_negate, "op_g2_negate"),
        (op_bls_map_to_g1, "op_g1_map"),
        (op_bls_map_to_g2, "op_g2_map"),
        (op_bls_pairing_identity, "op_bls_pairing_identity"),
        (op_bls_verify, "op_bls_verify"),
        (op_secp256k1_verify, "op_secp256k1_verify"),
        (op_secp256r1_verify, "op_secp256r1_verify"),
    ];
    let name: &[u8] = name.as_ref();
    for (f, op) in opcode_lookup.iter() {
        let pu8: &[u8] = op.as_ref();
        if pu8 == name {
            return Some(*f);
        }
    }
    None
}

pub fn f_lookup_for_hashmap(opcode_lookup_by_name: HashMap<String, Vec<u8>>) -> FLookup {
    let mut f_lookup = [None; 256];
    for (name, idx) in opcode_lookup_by_name.iter() {
        if idx.len() == 1 {
            let index = idx[0];
            let op = opcode_by_name(name);
            assert!(op.is_some(), "can't find native operator {name}");
            f_lookup[index as usize] = op;
        }
    }
    f_lookup
}
