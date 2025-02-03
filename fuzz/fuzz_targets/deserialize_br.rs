#![no_main]

mod node_eq;

use clvmr::allocator::Allocator;
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::serde::node_from_bytes_backrefs_old;
use clvmr::serde::node_to_bytes_backrefs;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let program = match node_from_bytes_backrefs(&mut allocator, data) {
        Err(_) => {
            return;
        }
        Ok(r) => r,
    };

    let b1 = node_to_bytes_backrefs(&allocator, program).unwrap();

    let program = node_from_bytes_backrefs(&mut allocator, &b1);

    let program_old = node_from_bytes_backrefs_old(&mut allocator, &b1);

    assert!(!(program.is_err() ^ program_old.is_err()));

    let program = program.unwrap();
    let program_old = program_old.unwrap();
    assert!(node_eq::node_eq(&allocator, program, program_old));
});
