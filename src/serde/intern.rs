//! CLVM tree interning: deduplicate atoms and pairs in a single pass.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use crate::allocator::{Allocator, Atom, NodePtr, SExp};
use crate::error::Result;

use super::bytes32::Bytes32;
use super::object_cache::{ObjectCache, treehash};

/// Result of interning a CLVM tree (deduplicated nodes, unique atoms/pairs).
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
    /// SHA256 tree hash (each unique node hashed once via ObjectCache).
    pub fn tree_hash(&self) -> [u8; 32] {
        let mut cache: ObjectCache<Bytes32> = ObjectCache::new(treehash);
        *cache
            .get_or_calculate(&self.allocator, &self.root, None)
            .expect("treehash should not fail on valid tree")
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
                let interned = match atom_to_interned.entry(atom) {
                    Entry::Occupied(o) => *o.get(),
                    Entry::Vacant(v) => {
                        let new_node = new_allocator.new_atom(atom.as_ref())?;
                        v.insert(new_node);
                        atoms.push(new_node);
                        new_node
                    }
                };
                node_to_interned.insert(current, interned);
            }
            SExp::Pair(left, right) => {
                // Check if children are processed
                let left_interned = node_to_interned.get(&left);
                let right_interned = node_to_interned.get(&right);

                if let (Some(l), Some(r)) = (left_interned, right_interned) {
                    // Both children processed, create or reuse pair
                    let interned = match pair_to_interned.entry((*l, *r)) {
                        Entry::Occupied(o) => *o.get(),
                        Entry::Vacant(v) => {
                            let new_node = new_allocator.new_pair(*l, *r)?;
                            v.insert(new_node);
                            pairs.push(new_node);
                            new_node
                        }
                    };
                    node_to_interned.insert(current, interned);
                } else {
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
        let SExp::Pair(left, right) = tree.allocator.sexp(inner_pair) else {
            panic!("Expected inner_pair to be a pair");
        };
        assert_eq!(tree.allocator.atom(left).as_ref(), &[2]);
        assert_eq!(tree.allocator.atom(right).as_ref(), &[3]);

        // Verify that outer_pair is actually the (A . (B . C)) pair
        let SExp::Pair(left, right) = tree.allocator.sexp(outer_pair) else {
            panic!("Expected outer_pair to be a pair");
        };
        assert_eq!(tree.allocator.atom(left).as_ref(), &[1]);
        assert_eq!(
            right, inner_pair,
            "Outer pair's right child should be the inner pair"
        );
    }
}
