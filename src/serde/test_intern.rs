use crate::allocator::{Allocator, NodePtr};
use crate::error::Result;
use crate::serde::bytes32::Bytes32;
use crate::serde::intern::intern;
use crate::serde::node_from_bytes_backrefs;
use crate::serde::node_to_bytes;
use crate::serde::object_cache::{ObjectCache, treehash};

fn treehash_for_node(allocator: &Allocator, node: NodePtr) -> Bytes32 {
    let mut object_cache = ObjectCache::new(treehash);
    *object_cache
        .get_or_calculate(allocator, &node, None)
        .unwrap()
}

/// Helper to convert hex string to bytes
fn hex_to_bytes(hex: &str) -> Vec<u8> {
    let hex_clean = hex.trim().replace([' ', '\n'], "");
    hex_clean
        .chars()
        .collect::<Vec<_>>()
        .chunks(2)
        .map(|chunk| {
            let s: String = chunk.iter().collect();
            u8::from_str_radix(&s, 16).expect("invalid hex")
        })
        .collect()
}

/// Helper to convert hex string directly to a node
fn hex_to_node(allocator: &mut Allocator, hex: &str) -> Result<crate::allocator::NodePtr> {
    let bytes = hex_to_bytes(hex);
    node_from_bytes_backrefs(allocator, &bytes)
}

/// Helper to deserialize hex and create interned version, returning intern stats
fn test_hex_interning(hex: &str, expected_atoms: usize, expected_pairs: usize) -> Result<()> {
    let mut allocator = Allocator::new();

    // Deserialize from hex
    let node = hex_to_node(&mut allocator, hex)?;

    // Create interned version using the new API
    let tree = intern(&allocator, node)?;

    // Ensure interned node serializes to same bytes
    let original_serialized = node_to_bytes(&allocator, node)?;
    let new_serialized = node_to_bytes(&tree.allocator, tree.root)?;
    assert_eq!(
        original_serialized, new_serialized,
        "Serialized bytes do not match after interning."
    );

    // Ensure treehashes match
    let original_treehash = treehash_for_node(&allocator, node);
    let new_treehash = tree.tree_hash();
    assert_eq!(
        original_treehash, new_treehash,
        "Treehashes do not match after interning."
    );

    // Verify unique atom and pair counts
    assert_eq!(
        tree.atoms.len(),
        expected_atoms,
        "Atom count doesn't match expected.\nGot:      {:?}\nExpected: {:?}",
        tree.atoms.len(),
        expected_atoms
    );
    assert_eq!(
        tree.pairs.len(),
        expected_pairs,
        "Pair count doesn't match expected.\nGot:      {:?}\nExpected: {:?}",
        tree.pairs.len(),
        expected_pairs
    );

    Ok(())
}

// ============================================================================
// Hex-based test cases with intern statistics verification
// ============================================================================

#[test]
fn test_interning() -> Result<()> {
    // Simple atom with value 1: 1 atom, 0 pairs
    test_hex_interning("01", 1, 0)?;

    // Atom with value 10: 1 atom, 0 pairs
    test_hex_interning("0a", 1, 0)?;

    // Pair of identical atoms (1 . 1): 1 atom (deduplicated), 1 pair
    test_hex_interning("ff0101", 1, 1)?;

    // Pair of different atoms (1 . 10): 2 atoms, 1 pair
    test_hex_interning("ff010a", 2, 1)?;

    // Nested structure (1 . (1 . 1)): 1 atom (deduplicated), 2 pairs
    test_hex_interning("ff01ff0101", 1, 2)?;

    // Nested structure ((42 . 42) . 42): 1 atom (42 deduplicated), 2 pairs
    test_hex_interning("ffff2a2a2a", 1, 2)?;

    // Deep nesting: (1 . (2 . (3 . 1))): 3 atoms (1,2,3 with 1 repeated), 3 pairs
    test_hex_interning("ff01ff02ff0301", 3, 3)?;

    // Three-element chain: (1 . (2 . (3 . nil))): 4 atoms (1,2,3,nil), 3 pairs
    test_hex_interning("ff01ff02ff0300", 4, 3)?;

    // Pair of different atoms at each level: (1 . (2 . (3 . 4)))
    test_hex_interning("ff01ff02ff0304", 4, 3)?;

    // Mixed atoms with one repeated: (1 . (2 . (1 . 3)))
    test_hex_interning("ff01ff02ff0103", 3, 3)?;

    Ok(())
}
