#![no_main]
use libfuzzer_sys::fuzz_target;
use clvm_rs::serialize::node_from_bytes;
use clvm_rs::allocator::Allocator;

fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let _program = match node_from_bytes(&mut allocator, data) {
        Err(_) => { return; },
        Ok(r) => r,
    };
});
