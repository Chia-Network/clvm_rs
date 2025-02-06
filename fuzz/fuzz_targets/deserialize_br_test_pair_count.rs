#![no_main]
use clvmr::allocator::Allocator;
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::serde::node_from_bytes_backrefs_old;
use clvmr::serde::node_to_bytes_backrefs;
use libfuzzer_sys::fuzz_target;

// #[cfg(feature = "counters")]
fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let cp = allocator.checkpoint();
    let program = match node_from_bytes_backrefs(&mut allocator, data) {
        Err(_) => {
            assert!(node_from_bytes_backrefs_old(&mut allocator, data).is_err());
            return;
        }
        Ok(r) => r,
    };

    let b1 = node_to_bytes_backrefs(&allocator, program).unwrap();

    // reset allocators
    allocator.restore_checkpoint(&cp);

    let _program = node_from_bytes_backrefs(&mut allocator, &b1).unwrap();
    let new_pair_count = allocator.pair_count();
    allocator.restore_checkpoint(&cp);
    let _program_old = node_from_bytes_backrefs_old(&mut allocator, &b1).unwrap();
    assert!(new_pair_count == allocator.pair_count());
});
