#![no_main]

mod make_tree;

use clvmr::allocator::Allocator;
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::serde::node_to_bytes_backrefs;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let mut unstructured = arbitrary::Unstructured::new(data);

    let (program, _) = make_tree::make_tree(&mut allocator, &mut unstructured);

    let b1 = node_to_bytes_backrefs(&allocator, program).unwrap();

    let mut allocator = Allocator::new();
    let program = node_from_bytes_backrefs(&mut allocator, &b1).unwrap();

    let b2 = node_to_bytes_backrefs(&allocator, program).unwrap();
    if b1 != b2 {
        panic!("b1 and b2 do not match");
    }
});
