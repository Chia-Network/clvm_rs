#![no_main]

mod fuzzing_utils;

use clvmr::allocator::Allocator;
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::serde::node_to_bytes_backrefs;
use libfuzzer_sys::fuzz_target;

fn do_fuzz(data: &[u8], short_atoms: bool) {
    let mut allocator = Allocator::new();
    let mut cursor = fuzzing_utils::BitCursor::new(data);

    let program = fuzzing_utils::make_tree(&mut allocator, &mut cursor, short_atoms);

    let b1 = node_to_bytes_backrefs(&allocator, program).unwrap();

    let mut allocator = Allocator::new();
    let program = node_from_bytes_backrefs(&mut allocator, &b1).unwrap();

    let b2 = node_to_bytes_backrefs(&allocator, program).unwrap();
    if b1 != b2 {
        panic!("b1 and b2 do not match");
    }
}

fuzz_target!(|data: &[u8]| {
    do_fuzz(data, true);
    do_fuzz(data, false);
});
