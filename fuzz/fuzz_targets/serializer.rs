#![no_main]

mod fuzzing_utils;

use clvmr::allocator::Allocator;
use clvmr::serde::{node_to_bytes_backrefs, Serializer};

use libfuzzer_sys::fuzz_target;

// serializing with the regular compressed serializer should yield the same
// result as using the incremental one (as long as it's in a single add() call).
fn do_fuzz(data: &[u8], short_atoms: bool) {
    let mut cursor = fuzzing_utils::BitCursor::new(data);

    let mut allocator = Allocator::new();
    let program = fuzzing_utils::make_tree(&mut allocator, &mut cursor, short_atoms);

    let b1 = node_to_bytes_backrefs(&allocator, program).unwrap();

    let mut ser = Serializer::new();
    let (done, _) = ser.add(&allocator, program, None).unwrap();
    assert!(done);
    let b2 = ser.into_inner();

    if b1 != b2 {
        panic!("b1 and b2 do not match");
    }
}

fuzz_target!(|data: &[u8]| {
    do_fuzz(data, true);
    do_fuzz(data, false);
});
