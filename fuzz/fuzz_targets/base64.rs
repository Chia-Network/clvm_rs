#![no_main]
use clvmr::base64_ops::{op_base64url_decode, op_base64url_encode};
use clvmr::{reduction::Reduction, Allocator, NodePtr};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut a = Allocator::new();
    let blob = a.new_atom(data).expect("failed to create atom");
    let args = a
        .new_pair(blob, NodePtr::NIL)
        .expect("failed to create pair");
    let Reduction(cost, node) =
        op_base64url_encode(&mut a, args, 11000000000).expect("base64url_encode failed");
    assert!(cost >= 170);

    let args = a
        .new_pair(node, NodePtr::NIL)
        .expect("failed to create pair");
    let Reduction(cost, node) =
        op_base64url_decode(&mut a, args, 11000000000).expect("base64url_decode failed");
    assert!(cost >= 400);
    assert!(a.atom_eq(node, blob));
});
