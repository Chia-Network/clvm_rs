#![no_main]

mod node_eq;

use clvmr::allocator::Allocator;
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::serde::node_from_bytes_backrefs_old;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let program = match node_from_bytes_backrefs(&mut allocator, data) {
        Err(_) => {
            assert!(node_from_bytes_backrefs_old(&mut allocator, data).is_err());
            return;
        }
        Ok(r) => r,
    };

    let program_old = node_from_bytes_backrefs_old(&mut allocator, data).unwrap();
    assert!(node_eq::node_eq(&allocator, program, program_old));
});
