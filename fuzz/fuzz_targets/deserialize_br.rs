#![no_main]
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

    let mut allocator = Allocator::new();
    let program = node_from_bytes_backrefs(&mut allocator, &b1).unwrap();
    let program_old = node_from_bytes_backrefs_old(&mut allocator, &b1).unwrap();
    let b2 = node_to_bytes_backrefs(&allocator, program).unwrap();
    assert_eq!(b1, b2);
    let b3 = node_to_bytes_backrefs(&allocator, program_old).unwrap();
    assert_eq!(b1, b3);
});

// #[cfg(feature = "counters")]
fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let cp = allocator.checkpoint();
    let program = match node_from_bytes_backrefs(&mut allocator, data) {
        Err(_) => {
            return;
        }
        Ok(r) => r,
    };

    let b1 = node_to_bytes_backrefs(&allocator, program).unwrap();

    // reset allocators
    allocator.restore_checkpoint(&cp);

    let program = node_from_bytes_backrefs(&mut allocator, &b1).unwrap();
    allocator.restore_checkpoint(&cp);
    let program_old = node_from_bytes_backrefs_old(&mut allocator_old, &b1).unwrap();
    assert!(allocator.pair_count() <= allocator_old.pair_count());
});
