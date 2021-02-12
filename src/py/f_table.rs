use std::collections::HashMap;

use crate::allocator::Allocator;
use crate::core_ops::{op_cons, op_eq, op_first, op_if, op_listp, op_raise, op_rest};
use crate::more_ops::{
    op_add, op_all, op_any, op_ash, op_concat, op_div, op_divmod, op_gr, op_gr_bytes, op_logand,
    op_logior, op_lognot, op_logxor, op_lsh, op_multiply, op_not, op_point_add, op_pubkey_for_exp,
    op_sha256, op_softfork, op_strlen, op_substr, op_subtract,
};
use crate::reduction::Response;

type OpFn<T> = fn(&mut T, <T as Allocator>::Ptr) -> Response<<T as Allocator>::Ptr>;

pub type FLookup<T> = [Option<OpFn<T>>; 256];

pub fn opcode_by_name<T: Allocator>(name: &str) -> Option<OpFn<T>> {
    let opcode_lookup: [(&str, OpFn<T>); 30] = [
        ("i", op_if),
        ("c", op_cons),
        ("f", op_first),
        ("r", op_rest),
        ("l", op_listp),
        ("x", op_raise),
        ("=", op_eq),
        ("sha256", op_sha256),
        ("+", op_add),
        ("-", op_subtract),
        ("*", op_multiply),
        ("divmod", op_divmod),
        ("substr", op_substr),
        ("strlen", op_strlen),
        ("point_add", op_point_add),
        ("pubkey_for_exp", op_pubkey_for_exp),
        ("concat", op_concat),
        (">", op_gr),
        (">s", op_gr_bytes),
        ("logand", op_logand),
        ("logior", op_logior),
        ("logxor", op_logxor),
        ("lognot", op_lognot),
        ("ash", op_ash),
        ("lsh", op_lsh),
        ("not", op_not),
        ("any", op_any),
        ("all", op_all),
        ("softfork", op_softfork),
        ("div", op_div),
    ];
    let name: &[u8] = name.as_ref();
    for (op, f) in opcode_lookup.iter() {
        let pu8: &[u8] = op.as_ref();
        if pu8 == name {
            return Some(*f);
        }
    }
    None
}

pub fn f_lookup_for_hashmap<A: Allocator>(
    opcode_lookup_by_name: HashMap<String, &[u8]>,
) -> FLookup<A> {
    let mut f_lookup = [None; 256];
    for (name, idx) in opcode_lookup_by_name.iter() {
        if idx.len() == 1 {
            let index = idx[0];
            let op = opcode_by_name(name);
            if op.is_none() {
                panic!("can't find native operator {:?}", name);
            }
            f_lookup[index as usize] = op;
        }
    }
    f_lookup
}
