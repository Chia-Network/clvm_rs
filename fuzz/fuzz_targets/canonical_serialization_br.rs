#![no_main]
mod make_tree;

use clvmr::serde::is_canonical_serialization;
use clvmr::serde::node_to_bytes_backrefs;
use clvmr::Allocator;
use libfuzzer_sys::fuzz_target;
use make_tree::make_tree_limits;

fuzz_target!(|data: &[u8]| {
    let mut unstructured = arbitrary::Unstructured::new(data);
    let mut a = Allocator::new();
    let (tree, _) = make_tree_limits(&mut a, &mut unstructured, 1000, true);

    let buffer = node_to_bytes_backrefs(&a, tree).expect("internal error, failed to serialize");

    // out serializer should always produce canonical serialization
    assert!(is_canonical_serialization(&buffer));
});
