//! CLVM tree interning - deduplicate atoms and pairs in a single pass.
//!
//! This module provides the core interning functionality for CLVM trees:
//! - Deduplicate identical atoms and pairs
//! - Collect unique nodes for cost calculation and serialization
//! - Compute tree hash efficiently over the interned structure

use std::collections::HashMap;

use crate::allocator::{Allocator, Atom, NodePtr, SExp};
use crate::error::Result;

use super::bytes32::Bytes32;
use super::object_cache::{ObjectCache, treehash};

/// Statistics from an interned tree - the building blocks for cost formulas.
///
/// These components can be combined in different ways depending on the cost
/// formula being used. The struct provides helper methods for common formulas.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InternedStats {
    /// Number of unique atoms
    pub atom_count: u64,
    /// Number of unique pairs
    pub pair_count: u64,
    /// Sum of all unique atom byte lengths: Σ(atom_len)
    pub atom_bytes: u64,
    /// SHA256 blocks for atoms: Σ(⌈(atom_len + 10) / 64⌉)
    /// The +10 accounts for: 0x01 prefix (1 byte) + SHA256 padding overhead (9 bytes)
    pub sha_atom_blocks: u64,
}

impl InternedStats {
    /// Total unique nodes (atoms + pairs)
    #[inline]
    pub fn node_count(&self) -> u64 {
        self.atom_count + self.pair_count
    }

    /// SHA256 blocks for pairs: always 2 per pair.
    /// Each pair hashes: 0x02 (1) + left_hash (32) + right_hash (32) = 65 bytes
    /// With padding: 74 bytes → always 2 SHA256 blocks
    #[inline]
    pub fn sha_pair_blocks(&self) -> u64 {
        2 * self.pair_count
    }

    /// Total SHA256 blocks needed for tree hashing (atom blocks + pair blocks)
    #[inline]
    pub fn sha_blocks(&self) -> u64 {
        self.sha_atom_blocks + self.sha_pair_blocks()
    }

    /// Total SHA256 invocations needed (one per unique node)
    #[inline]
    pub fn sha_invocations(&self) -> u64 {
        self.atom_count + self.pair_count
    }
}

/// Result of interning a CLVM tree.
///
/// Contains the deduplicated tree structure and lists of unique nodes,
/// enabling efficient cost calculation, tree hashing, and serialization.
#[derive(Debug)]
pub struct InternedTree {
    /// Allocator containing only unique (deduplicated) nodes
    pub allocator: Allocator,
    /// Root node in the interned allocator
    pub root: NodePtr,
    /// All unique atoms, in insertion order
    pub atoms: Vec<NodePtr>,
    /// All unique pairs, in post-order (children before parents)
    pub pairs: Vec<NodePtr>,
}

impl InternedTree {
    /// Compute statistics for this interned tree.
    ///
    /// This is O(atoms.len()) - it iterates the atom list once to sum byte lengths.
    pub fn stats(&self) -> InternedStats {
        let mut stats = InternedStats {
            atom_count: self.atoms.len() as u64,
            pair_count: self.pairs.len() as u64,
            atom_bytes: 0,
            sha_atom_blocks: 0,
        };

        for &atom in &self.atoms {
            let len = self.allocator.atom_len(atom) as u64;
            stats.atom_bytes += len;
            // SHA256 blocks: ceil((len + 10) / 64) = (len + 73) / 64
            stats.sha_atom_blocks += (len + 73) / 64;
        }

        stats
    }

    /// Compute SHA256 tree hash for the interned tree.
    ///
    /// This is efficient because each unique node is only hashed once,
    /// and the ObjectCache handles memoization automatically.
    pub fn tree_hash(&self) -> Bytes32 {
        let mut cache: ObjectCache<Bytes32> = ObjectCache::new(treehash);
        *cache
            .get_or_calculate(&self.allocator, &self.root, None)
            .expect("treehash should not fail on valid tree")
    }

    /// Get a mapping from NodePtr to index for serialization.
    ///
    /// Returns (atom_to_index, pair_to_index) where:
    /// - Atom indices are 0, 1, 2, ... (non-negative)
    /// - Pair indices are -1, -2, -3, ... (negative, 1-based)
    ///
    /// This is useful for serialization formats that reference nodes by index.
    pub fn node_indices(&self) -> (HashMap<NodePtr, i32>, HashMap<NodePtr, i32>) {
        let mut atom_to_index = HashMap::with_capacity(self.atoms.len());
        let mut pair_to_index = HashMap::with_capacity(self.pairs.len());

        for (i, &atom) in self.atoms.iter().enumerate() {
            atom_to_index.insert(atom, i as i32);
        }
        for (i, &pair) in self.pairs.iter().enumerate() {
            pair_to_index.insert(pair, -(i as i32 + 1));
        }

        (atom_to_index, pair_to_index)
    }
}

/// Intern a CLVM tree: deduplicate atoms and pairs in a single pass.
///
/// This function traverses the source tree once, building a new allocator
/// with deduplicated nodes. It tracks:
/// - Atoms by content (identical byte sequences share one node)
/// - Pairs by their (left, right) tuple in the interned allocator
///
/// The resulting `InternedTree` contains:
/// - A new allocator with only unique nodes
/// - The root node in the new allocator
/// - Lists of unique atoms and pairs for cost/serialization
///
/// # Algorithm
///
/// Uses an iterative post-order traversal with explicit stack:
/// 1. Push root to stack
/// 2. For each node:
///    - If atom: deduplicate by content, add to atoms list if new
///    - If pair: wait for children to be processed, then deduplicate by (left, right)
/// 3. Pairs are naturally collected in post-order (children before parents)
///
/// # Errors
///
/// Returns an error if allocator limits are exceeded when creating new nodes.
pub fn intern(allocator: &Allocator, node: NodePtr) -> Result<InternedTree> {
    let mut new_allocator = Allocator::new();
    let mut atoms: Vec<NodePtr> = Vec::new();
    let mut pairs: Vec<NodePtr> = Vec::new();

    // Maps from source allocator to interned allocator
    let mut node_to_interned: HashMap<NodePtr, NodePtr> = HashMap::new();
    // Maps atom content to interned NodePtr (for deduplication)
    let mut atom_to_interned: HashMap<Atom, NodePtr> = HashMap::new();
    // Maps (left_interned, right_interned) to interned pair NodePtr
    let mut pair_to_interned: HashMap<(NodePtr, NodePtr), NodePtr> = HashMap::new();

    let mut stack = vec![node];

    while let Some(current) = stack.pop() {
        // Skip if already processed
        if node_to_interned.contains_key(&current) {
            continue;
        }

        match allocator.sexp(current) {
            SExp::Atom => {
                let atom = allocator.atom(current);
                let interned = if let Some(&existing) = atom_to_interned.get(atom.as_ref()) {
                    existing
                } else {
                    let new_node = new_allocator.new_atom(atom.as_ref())?;
                    atom_to_interned.insert(atom, new_node);
                    atoms.push(new_node);
                    new_node
                };
                node_to_interned.insert(current, interned);
            }
            SExp::Pair(left, right) => {
                // Check if children are processed
                let left_interned = node_to_interned.get(&left);
                let right_interned = node_to_interned.get(&right);

                match (left_interned, right_interned) {
                    (Some(&l), Some(&r)) => {
                        // Both children processed, create or reuse pair
                        let interned = if let Some(&existing) = pair_to_interned.get(&(l, r)) {
                            existing
                        } else {
                            let new_node = new_allocator.new_pair(l, r)?;
                            pair_to_interned.insert((l, r), new_node);
                            pairs.push(new_node);
                            new_node
                        };
                        node_to_interned.insert(current, interned);
                    }
                    _ => {
                        // Need to process children first
                        stack.push(current);
                        if right_interned.is_none() {
                            stack.push(right);
                        }
                        if left_interned.is_none() {
                            stack.push(left);
                        }
                    }
                }
            }
        }
    }

    let root = node_to_interned[&node];
    Ok(InternedTree {
        allocator: new_allocator,
        root,
        atoms,
        pairs,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde::node_from_bytes;

    #[test]
    fn test_intern_single_atom() {
        let mut allocator = Allocator::new();
        let node = allocator.new_atom(&[1, 2, 3]).unwrap();

        let tree = intern(&allocator, node).unwrap();

        assert_eq!(tree.atoms.len(), 1);
        assert_eq!(tree.pairs.len(), 0);
        assert_eq!(tree.allocator.atom(tree.root).as_ref(), &[1, 2, 3]);
    }

    #[test]
    fn test_intern_simple_pair() {
        let mut allocator = Allocator::new();
        let left = allocator.new_atom(&[1]).unwrap();
        let right = allocator.new_atom(&[2]).unwrap();
        let node = allocator.new_pair(left, right).unwrap();

        let tree = intern(&allocator, node).unwrap();

        assert_eq!(tree.atoms.len(), 2);
        assert_eq!(tree.pairs.len(), 1);
    }

    #[test]
    fn test_intern_deduplicates_atoms() {
        // Create (A . A) where A has same content
        let mut allocator = Allocator::new();
        let a1 = allocator.new_atom(&[42]).unwrap();
        let a2 = allocator.new_atom(&[42]).unwrap(); // Same content, different NodePtr
        let node = allocator.new_pair(a1, a2).unwrap();

        let tree = intern(&allocator, node).unwrap();

        // Should have only 1 unique atom
        assert_eq!(tree.atoms.len(), 1);
        assert_eq!(tree.pairs.len(), 1);
    }

    #[test]
    fn test_intern_deduplicates_pairs() {
        // Create ((A . B) . (A . B))
        let mut allocator = Allocator::new();
        let a = allocator.new_atom(&[1]).unwrap();
        let b = allocator.new_atom(&[2]).unwrap();
        let p1 = allocator.new_pair(a, b).unwrap();
        let p2 = allocator.new_pair(a, b).unwrap(); // Same structure, different NodePtr
        let node = allocator.new_pair(p1, p2).unwrap();

        let tree = intern(&allocator, node).unwrap();

        // Should have 2 atoms, 2 pairs (inner pair deduplicated)
        assert_eq!(tree.atoms.len(), 2);
        assert_eq!(tree.pairs.len(), 2); // (A . B) and ((A.B) . (A.B))
    }

    #[test]
    fn test_stats() {
        let mut allocator = Allocator::new();
        let a = allocator.new_atom(&[1, 2, 3, 4, 5]).unwrap(); // 5 bytes
        let b = allocator.new_atom(&[6, 7, 8]).unwrap(); // 3 bytes
        let node = allocator.new_pair(a, b).unwrap();

        let tree = intern(&allocator, node).unwrap();
        let stats = tree.stats();

        assert_eq!(stats.atom_count, 2);
        assert_eq!(stats.pair_count, 1);
        assert_eq!(stats.atom_bytes, 8);
        assert_eq!(stats.sha_pair_blocks(), 2);
    }

    #[test]
    fn test_tree_hash_deterministic() {
        let mut alloc1 = Allocator::new();
        let a1 = alloc1.new_atom(&[1, 2, 3]).unwrap();
        let b1 = alloc1.new_atom(&[4, 5, 6]).unwrap();
        let node1 = alloc1.new_pair(a1, b1).unwrap();

        let mut alloc2 = Allocator::new();
        let a2 = alloc2.new_atom(&[1, 2, 3]).unwrap();
        let b2 = alloc2.new_atom(&[4, 5, 6]).unwrap();
        let node2 = alloc2.new_pair(a2, b2).unwrap();

        let tree1 = intern(&alloc1, node1).unwrap();
        let tree2 = intern(&alloc2, node2).unwrap();

        assert_eq!(tree1.tree_hash(), tree2.tree_hash());
    }

    #[test]
    fn test_pairs_in_post_order() {
        // Create (A . (B . C))
        let mut allocator = Allocator::new();
        let a = allocator.new_atom(&[1]).unwrap();
        let b = allocator.new_atom(&[2]).unwrap();
        let c = allocator.new_atom(&[3]).unwrap();
        let inner = allocator.new_pair(b, c).unwrap();
        let outer = allocator.new_pair(a, inner).unwrap();

        let tree = intern(&allocator, outer).unwrap();

        // Post-order: inner pair before outer pair
        assert_eq!(tree.pairs.len(), 2);
        // The inner pair (B . C) should come before the outer pair (A . (B . C))
        // because children must be processed before parents

        // Verify the ordering: inner pair should be first, outer pair should be second
        let inner_pair = tree.pairs[0];
        let outer_pair = tree.pairs[1];

        // Verify that inner_pair is actually the (B . C) pair
        match tree.allocator.sexp(inner_pair) {
            SExp::Pair(left, right) => {
                assert_eq!(tree.allocator.atom(left).as_ref(), &[2]);
                assert_eq!(tree.allocator.atom(right).as_ref(), &[3]);
            }
            _ => panic!("Expected inner_pair to be a pair"),
        }

        // Verify that outer_pair is actually the (A . (B . C)) pair
        match tree.allocator.sexp(outer_pair) {
            SExp::Pair(left, right) => {
                assert_eq!(tree.allocator.atom(left).as_ref(), &[1]);
                assert_eq!(
                    right, inner_pair,
                    "Outer pair's right child should be the inner pair"
                );
            }
            _ => panic!("Expected outer_pair to be a pair"),
        }
    }

    #[test]
    fn test_stats_values() {
        let mut allocator = Allocator::new();
        // 2 atoms (10 bytes total) and 3 pairs
        let a = allocator.new_atom(&[1, 2, 3, 4, 5]).unwrap();
        let b = allocator.new_atom(&[6, 7, 8, 9, 10]).unwrap();
        let p1 = allocator.new_pair(a, b).unwrap();
        let p2 = allocator.new_pair(p1, a).unwrap();
        let p3 = allocator.new_pair(p2, b).unwrap();

        let tree = intern(&allocator, p3).unwrap();
        let stats = tree.stats();

        assert_eq!(stats.atom_count, 2);
        assert_eq!(stats.pair_count, 3);
        assert_eq!(stats.atom_bytes, 10);
        assert_eq!(stats.node_count(), 5);
        assert_eq!(stats.sha_invocations(), 5);
    }

    #[test]
    fn test_from_serialized_bytes() {
        // ff8568656c6c6f85776f726c64 = ("hello" . "world")
        let bytes = hex::decode("ff8568656c6c6f85776f726c64").unwrap();
        let mut allocator = Allocator::new();
        let node = node_from_bytes(&mut allocator, &bytes).unwrap();

        let tree = intern(&allocator, node).unwrap();
        let stats = tree.stats();

        assert_eq!(stats.atom_count, 2);
        assert_eq!(stats.pair_count, 1);
        assert_eq!(stats.atom_bytes, 10); // "hello" (5) + "world" (5)
    }
}
