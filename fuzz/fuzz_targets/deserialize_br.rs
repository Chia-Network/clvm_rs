#![no_main]

use chia_fuzzing::node_eq;
use clvmr::allocator::Allocator;
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::serde::node_from_bytes_backrefs_old;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let res1 = node_from_bytes_backrefs(&mut allocator, data);
    let node_count = allocator.pair_count();
    let res2 = node_from_bytes_backrefs_old(&mut allocator, data);
    // check that the new implementation creates the same number of pair nodes as the old one
    assert_eq!(node_count * 2, allocator.pair_count());
    match (res1, res2) {
        (Err(_e1), Err(_e2)) => {
            // both failed, that's fine
            return;
        }
        (Ok(n1), Ok(n2)) => {
            assert!(node_eq(&allocator, n1, n2));
        }
        _ => {
            panic!("mismatching results");
        }
    }
});
