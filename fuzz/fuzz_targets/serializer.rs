#![no_main]

mod make_tree;
mod node_eq;

use clvmr::allocator::Allocator;
use clvmr::serde::{node_from_bytes_backrefs, node_to_bytes_backrefs, Serializer};
use node_eq::node_eq;

use libfuzzer_sys::fuzz_target;

// serializing with the regular compressed serializer should yield the same
// result as using the incremental one (as long as it's in a single add() call).
fuzz_target!(|data: &[u8]| {
    let mut unstructured = arbitrary::Unstructured::new(data);
    let mut allocator = Allocator::new();
    let program = make_tree::make_tree(&mut allocator, &mut unstructured);

    let b1 = node_to_bytes_backrefs(&allocator, program).unwrap();

    let mut ser = Serializer::new(None);
    let (done, _) = ser.add(&allocator, program).unwrap();
    assert!(done);
    let b2 = ser.into_inner();

    {
        // make sure both serializations are valid, and can be parsed to produce
        // the same tree
        let b1 = node_from_bytes_backrefs(&mut allocator, &b1).unwrap();
        let b2 = node_from_bytes_backrefs(&mut allocator, &b2).unwrap();
        assert!(node_eq(&allocator, b1, program));
        assert!(node_eq(&allocator, b1, b2));
    }

    assert_eq!(b1, b2);
});
