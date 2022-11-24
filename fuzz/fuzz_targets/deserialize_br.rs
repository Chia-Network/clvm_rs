#![no_main]
use clvmr::allocator::Allocator;
use clvmr::serde::node_from_bytes_backrefs;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let _program = match node_from_bytes_backrefs(&mut allocator, data) {
        Err(_) => {
            return;
        }
        Ok(r) => r,
    };
});
