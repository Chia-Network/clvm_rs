use crate::allocator::Allocator;
use crate::core_ops::{op_cons, op_eq, op_first, op_if, op_listp, op_raise, op_rest};
use crate::more_ops::{
    op_add, op_all, op_any, op_ash, op_concat, op_div, op_divmod, op_gr, op_gr_bytes, op_logand,
    op_logior, op_lognot, op_logxor, op_lsh, op_multiply, op_not, op_point_add, op_pubkey_for_exp,
    op_sha256, op_softfork, op_strlen, op_substr, op_subtract,
};
use crate::node::Node;
use crate::reduction::Response;

type OpFn<T> = fn(&Node<T>) -> Response<<T as Allocator>::Ptr>;

pub type FLookup<T> = [Option<OpFn<T>>; 256];

pub fn make_f_lookup<T: Allocator>() -> FLookup<T> {
    let opcode_lookup: [(u8, OpFn<T>); 30] = [
        (4, op_if),
        (5, op_cons),
        (6, op_first),
        (7, op_rest),
        (8, op_listp),
        (9, op_raise),
        (10, op_eq),
        (11, op_sha256),
        (12, op_add),
        (13, op_subtract),
        (14, op_multiply),
        (15, op_divmod),
        (16, op_substr),
        (17, op_strlen),
        (18, op_point_add),
        (19, op_pubkey_for_exp),
        (20, op_concat),
        (22, op_gr),
        (23, op_gr_bytes),
        (24, op_logand),
        (25, op_logior),
        (26, op_logxor),
        (27, op_lognot),
        (28, op_ash),
        (29, op_lsh),
        (30, op_not),
        (31, op_any),
        (32, op_all),
        (33, op_softfork),
        (34, op_div),
    ];
    let mut f_lookup: FLookup<T> = [None; 256];
    for (op, f) in &opcode_lookup {
        f_lookup[*op as usize] = Some(*f);
    }

    f_lookup
}
