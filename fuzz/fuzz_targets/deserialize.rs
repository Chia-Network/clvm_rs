#![no_main]
use clvmr::allocator::Allocator;
use clvmr::serde::{node_from_bytes, node_to_bytes};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let Ok(node) = node_from_bytes(&mut allocator, data) else {
        return;
    };
    // Any buffer the plain deserializer accepts must round-trip 1:1.
    let buffer = node_to_bytes(&allocator, node).expect("failed to serialize");
    assert_eq!(buffer, data);
});
