#![no_main]

use clvm_fuzzing::ArbitraryClvmTree;
use clvmr::serde::is_canonical_serialization;
use clvmr::serde::node_to_bytes_backrefs;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|tree: ArbitraryClvmTree<1000, true>| {
    let buffer = node_to_bytes_backrefs(&tree.allocator, tree.tree)
        .expect("internal error, failed to serialize");
    // out serializer should always produce canonical serialization
    assert!(is_canonical_serialization(&buffer));
});
