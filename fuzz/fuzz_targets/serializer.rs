#![no_main]

use clvm_fuzzing::{ArbitraryClvmTree, node_eq};
use clvmr::serde::{Serializer, node_from_bytes_backrefs, node_to_bytes_backrefs};

use libfuzzer_sys::fuzz_target;

// serializing with the regular compressed serializer should yield the same
// result as using the incremental one (as long as it's in a single add() call).
fuzz_target!(|program: ArbitraryClvmTree| {
    let mut a = program.allocator;
    let b1 = node_to_bytes_backrefs(&a, program.tree).unwrap();

    let mut ser = Serializer::new(None);
    let (done, _) = ser.add(&a, program.tree).unwrap();
    assert!(done);
    let b2 = ser.into_inner();

    // make sure both serializations are valid, and can be parsed to produce
    // the same tree
    let b1 = node_from_bytes_backrefs(&mut a, &b1).unwrap();
    let b2 = node_from_bytes_backrefs(&mut a, &b2).unwrap();
    assert!(node_eq(&a, b1, program.tree));
    assert!(node_eq(&a, b1, b2));
});
