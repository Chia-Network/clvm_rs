#![no_main]
use libfuzzer_sys::fuzz_target;
use clvmr::serialize::node_from_bytes;
use clvmr::allocator::Allocator;

fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let _program = match node_from_bytes(&mut allocator, data) {
        Err(_) => { return; },
        Ok(r) => r,
    };
});
