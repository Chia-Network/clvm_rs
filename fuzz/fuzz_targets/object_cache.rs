#![no_main]
mod fuzzing_utils;

use clvmr::serde::{node_to_bytes, serialized_length, treehash, ObjectCache};
use clvmr::Allocator;
use libfuzzer_sys::fuzz_target;

use fuzzing_utils::{make_tree, tree_hash, visit_tree, BitCursor};

fn do_fuzz(data: &[u8], short_atoms: bool) {
    let mut cursor = BitCursor::new(data);
    let mut allocator = Allocator::new();
    let program = make_tree(&mut allocator, &mut cursor, short_atoms);

    let mut hash_cache = ObjectCache::new(treehash);
    let mut length_cache = ObjectCache::new(serialized_length);
    visit_tree(&allocator, program, |a, node| {
        let expect_hash = tree_hash(a, node);
        let expect_len = node_to_bytes(a, node).unwrap().len() as u64;
        let computed_hash = hash_cache.get_or_calculate(a, &node, None).unwrap();
        let computed_len = length_cache.get_or_calculate(a, &node, None).unwrap();
        assert_eq!(computed_hash, &expect_hash);
        assert_eq!(computed_len, &expect_len);
    });
}

fuzz_target!(|data: &[u8]| {
    do_fuzz(data, true);
    do_fuzz(data, false);
});
