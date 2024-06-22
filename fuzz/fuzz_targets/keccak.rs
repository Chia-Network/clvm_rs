#![no_main]
use clvmr::keccak256_ops::op_keccak256;
use clvmr::{reduction::Reduction, Allocator, NodePtr};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut a = Allocator::new();
    let blob = a.new_atom(data).expect("failed to create atom");
    let args = a
        .new_pair(blob, NodePtr::NIL)
        .expect("failed to create pair");
    let Reduction(cost, node) = op_keccak256(&mut a, args, 11000000000).expect("keccak256 failed");
    assert!(cost >= 210);
    assert!(node.is_atom());
    assert_eq!(a.atom_len(node), 32);
});
