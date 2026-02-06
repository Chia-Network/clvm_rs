#![no_main]

use clvm_fuzzing::make_tree_limits;
use clvmr::allocator::Allocator;
use clvmr::serde::{ObjectCache, intern, node_to_bytes, treehash};
use libfuzzer_sys::fuzz_target;

// Fuzzer for the interning functionality.
// Build and run with allocator-debug enabled (default for this fuzz crate) so NodePtr
// don't get mixed up between the source and interned allocators.
// Verifies that:
// 1. Interning succeeds on valid nodes
// 2. The interned node serializes to the same bytes as the original
// 3. The tree hash is preserved
// 4. Interned nodes have fewer or equal unique atoms/pairs (deduplication works)
fuzz_target!(|data: &[u8]| {
    let mut unstructured = arbitrary::Unstructured::new(data);
    let mut allocator = Allocator::new();
    let (program, _) =
        make_tree_limits(&mut allocator, &mut unstructured, 600_000, false).expect("out of memory");

    // Serialize the original node
    let original_serialized = match node_to_bytes(&allocator, program) {
        Ok(b) => b,
        Err(_) => return,
    };

    // Compute original tree hash
    let mut original_cache = ObjectCache::new(treehash);
    let original_tree_hash = match original_cache.get_or_calculate(&allocator, &program, None) {
        Some(hash) => *hash,
        None => return,
    };

    // Count original atoms and pairs before interning
    let original_atoms = allocator.atom_count() + allocator.small_atom_count();
    let original_pairs = allocator.pair_count_no_ghosts();

    // Create interned version using new API
    let tree = match intern(&allocator, program) {
        Ok(result) => result,
        Err(_) => return,
    };

    // Serialize the interned node
    let interned_serialized = match node_to_bytes(&tree.allocator, tree.root) {
        Ok(b) => b,
        Err(_) => panic!("Interned node should serialize successfully"),
    };

    // The serializations must match
    assert_eq!(
        original_serialized, interned_serialized,
        "Serialized bytes differ after interning"
    );

    // Get stats and verify deduplication
    let stats = tree.stats();

    // Interning should not increase atom/pair counts (deduplication)
    assert!(
        stats.atom_count as usize <= original_atoms,
        "Interning increased atoms: {} -> {}",
        original_atoms,
        stats.atom_count
    );
    assert!(
        stats.pair_count as usize <= original_pairs,
        "Interning increased pairs: {} -> {}",
        original_pairs,
        stats.pair_count
    );

    // Verify tree hash is preserved
    let interned_tree_hash = tree.tree_hash();
    assert_eq!(
        original_tree_hash, interned_tree_hash,
        "Tree hash differs after interning"
    );
});
