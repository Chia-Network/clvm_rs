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
    let Ok(original_serialized) = node_to_bytes(&allocator, program) else {
        return;
    };

    // Compute original tree hash
    let mut original_cache = ObjectCache::new(treehash);
    let Some(original_tree_hash) = original_cache.get_or_calculate(&allocator, &program, None)
    else {
        return;
    };
    let original_tree_hash = *original_tree_hash;

    // Count original atoms and pairs before interning
    let original_atoms = allocator.atom_count();
    let original_pairs = allocator.pair_count();
    let original_allocated_atoms = allocator.allocated_atom_count();
    let original_allocated_pairs = allocator.allocated_pair_count();

    // Create interned version using new API
    let Ok(tree) = intern(&allocator, program) else {
        return;
    };

    // Serialize the interned node
    let Ok(interned_serialized) = node_to_bytes(&tree.allocator, tree.root) else {
        panic!("Interned node should serialize successfully");
    };

    // The serializations must match
    assert_eq!(
        original_serialized, interned_serialized,
        "Serialized bytes differ after interning"
    );

    // Verify deduplication: interned unique counts should not exceed original
    let interned_atoms = tree.atoms.len();
    let interned_pairs = tree.pairs.len();
    assert!(
        interned_atoms <= original_atoms,
        "Interning increased atoms: {original_atoms} -> {interned_atoms}"
    );
    assert!(
        interned_pairs <= original_pairs,
        "Interning increased pairs: {original_pairs} -> {interned_pairs}",
    );

    // Verify allocated counts (RAM usage) do not increase
    let interned_allocated_atoms = tree.allocator.allocated_atom_count();
    let interned_allocated_pairs = tree.allocator.allocated_pair_count();
    assert!(
        interned_allocated_atoms <= original_allocated_atoms,
        "Interning increased allocated atoms: {original_allocated_atoms} -> {interned_allocated_atoms}",
    );
    assert!(
        interned_allocated_pairs <= original_allocated_pairs,
        "Interning increased allocated pairs: {original_allocated_pairs} -> {interned_allocated_pairs}",
    );

    // Verify tree hash is preserved
    let interned_tree_hash = tree.tree_hash();
    assert_eq!(
        original_tree_hash, interned_tree_hash,
        "Tree hash differs after interning"
    );
});
