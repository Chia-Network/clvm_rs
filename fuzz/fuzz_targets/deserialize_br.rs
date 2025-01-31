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
    let cp = allocator.checkpoint();
    let program = node_from_bytes_backrefs(&mut allocator, &b1).unwrap();
    let new_pair_count = allocator.pair_count();
    allocator.restore_checkpoint(&cp);
    let program_old = node_from_bytes_backrefs_old(&mut allocator, &b1).unwrap();
    // check we aren't making more nodes in the new version
    assert!(new_pair_count <= allocator.pair_count());
    // check that the two versions of the deserializer produce the same/correct result
    let b2 = node_to_bytes_backrefs(&allocator, program).unwrap();
    assert_eq!(b1, b2);
    let b3 = node_to_bytes_backrefs(&allocator, program_old).unwrap();
    assert_eq!(b1, b3);
});
