use crate::allocator::NodeVisitor;
use crate::allocator::{Allocator, NodePtr};
use crate::cost::check_cost;
use crate::cost::Cost;
use crate::more_ops::PRECOMPUTED_HASHES;
use crate::op_utils::MALLOC_COST_PER_BYTE;
use crate::reduction::Reduction;
use crate::reduction::Response;
use crate::ObjectType;
use crate::SExp;
use chia_sha2::Sha256;

const SHA256TREE_BASE_COST: Cost = 0;
const SHA256TREE_COST_PER_CALL: Cost = 1300;
const SHA256TREE_COST_PER_BYTE: Cost = 10;

pub fn tree_hash_atom(bytes: &[u8]) -> [u8; 32] {
    let mut sha256 = Sha256::new();
    sha256.update([1]);
    sha256.update(bytes);
    sha256.finalize()
}

pub fn tree_hash_pair(first: [u8; 32], rest: [u8; 32]) -> [u8; 32] {
    let mut sha256 = Sha256::new();
    sha256.update([2]);
    sha256.update(first);
    sha256.update(rest);
    sha256.finalize()
}

#[derive(Default)]
pub struct TreeCache {
    hashes: Vec<[u8; 32]>,
    // parallel vector holding the cost used to compute the corresponding hash
    costs: Vec<Cost>,
    // each entry is an index into hashes and costs, or one of 3 special values:
    // u16::MAX if the pair has not been visited
    // u16::MAX - 1 if the pair has been seen once
    // u16::MAX - 2 if the pair has been seen at least twice (this makes it a
    // candidate for memoization)
    pairs: Vec<u16>,
}

const NOT_VISITED: u16 = u16::MAX;
const SEEN_ONCE: u16 = u16::MAX - 1;
const SEEN_MULTIPLE: u16 = u16::MAX - 2;

impl TreeCache {
    /// Get cached hash and its associated cost (if present).
    pub fn get(&self, n: NodePtr) -> Option<(&[u8; 32], Cost)> {
        // We only cache pairs (for now)
        if !matches!(n.object_type(), ObjectType::Pair) {
            return None;
        }

        let idx = n.index() as usize;
        let slot = *self.pairs.get(idx)?;
        if slot >= SEEN_MULTIPLE {
            return None;
        }
        Some((&self.hashes[slot as usize], self.costs[slot as usize]))
    }

    /// Insert a cached hash with its associated cost. If the cache is full we
    /// ignore the insertion.
    pub fn insert(&mut self, n: NodePtr, hash: &[u8; 32], cost: Cost) {
        // If we've reached the max size, just ignore new cache items
        if self.hashes.len() == SEEN_MULTIPLE as usize {
            return;
        }

        if !matches!(n.object_type(), ObjectType::Pair) {
            return;
        }

        let idx = n.index() as usize;
        if idx >= self.pairs.len() {
            self.pairs.resize(idx + 1, NOT_VISITED);
        }

        let slot = self.hashes.len();
        self.hashes.push(*hash);
        self.costs.push(cost);
        self.pairs[idx] = slot as u16;
    }

    /// mark the node as being visited. Returns true if we need to
    /// traverse visitation down this node.
    fn visit(&mut self, n: NodePtr) -> bool {
        if !matches!(n.object_type(), ObjectType::Pair) {
            return false;
        }
        let idx = n.index() as usize;
        if idx >= self.pairs.len() {
            self.pairs.resize(idx + 1, NOT_VISITED);
        }
        if self.pairs[idx] > SEEN_MULTIPLE {
            self.pairs[idx] -= 1;
        }
        self.pairs[idx] == SEEN_ONCE
    }

    pub fn should_memoize(&mut self, n: NodePtr) -> bool {
        if !matches!(n.object_type(), ObjectType::Pair) {
            return false;
        }
        let idx = n.index() as usize;
        if idx >= self.pairs.len() {
            false
        } else {
            self.pairs[idx] <= SEEN_MULTIPLE
        }
    }

    pub fn visit_tree(&mut self, a: &Allocator, node: NodePtr) {
        if !self.visit(node) {
            return;
        }
        let mut nodes = vec![node];
        while let Some(n) = nodes.pop() {
            let SExp::Pair(left, right) = a.sexp(n) else {
                continue;
            };
            if self.visit(left) {
                nodes.push(left);
            }
            if self.visit(right) {
                nodes.push(right);
            }
        }
    }
}

enum TreeOp {
    SExp(NodePtr),
    Cons,
    ConsAddCacheCost(NodePtr, Cost),
}

pub fn tree_hash_cached_costed(
    a: &mut Allocator,
    node: NodePtr,
    cache: &mut TreeCache,
    cost_left: Cost,
) -> Response {
    cache.visit_tree(a, node);

    let mut hashes = Vec::new();
    let mut ops = vec![TreeOp::SExp(node)];
    let mut cost: Cost = SHA256TREE_BASE_COST;

    while let Some(op) = ops.pop() {
        match op {
            TreeOp::SExp(node) => {
                // charge a call cost for processing this op
                cost += SHA256TREE_COST_PER_CALL;
                check_cost(cost, cost_left)?;
                match a.node(node) {
                    NodeVisitor::Buffer(bytes) => {
                        cost += SHA256TREE_COST_PER_BYTE * bytes.len() as u64;
                        check_cost(cost, cost_left)?;
                        let hash = tree_hash_atom(bytes);
                        hashes.push(hash);
                    }
                    NodeVisitor::U32(val) => {
                        cost += SHA256TREE_COST_PER_BYTE * a.atom_len(node) as u64;
                        check_cost(cost, cost_left)?;
                        if (val as usize) < PRECOMPUTED_HASHES.len() {
                            hashes.push(PRECOMPUTED_HASHES[val as usize]);
                        } else {
                            hashes.push(tree_hash_atom(a.atom(node).as_ref()));
                        }
                    }
                    NodeVisitor::Pair(left, right) => {
                        cost += SHA256TREE_COST_PER_BYTE * 65_u64;
                        check_cost(cost, cost_left)?;
                        if let Some((hash, cached_cost)) = cache.get(node) {
                            // when reusing a cached subtree, charge its cached cost
                            cost += cached_cost;
                            check_cost(cost, cost_left)?;
                            hashes.push(*hash);
                        } else {
                            if cache.should_memoize(node) {
                                // record the cost before traversing this subtree
                                ops.push(TreeOp::ConsAddCacheCost(node, cost));
                            } else {
                                ops.push(TreeOp::Cons);
                            }
                            ops.push(TreeOp::SExp(left));
                            ops.push(TreeOp::SExp(right));
                        }
                    }
                }
            }
            TreeOp::Cons => {
                let first = hashes.pop().unwrap();
                let rest = hashes.pop().unwrap();
                hashes.push(tree_hash_pair(first, rest));
            }
            TreeOp::ConsAddCacheCost(original_node, cost_before) => {
                let first = hashes.pop().unwrap();
                let rest = hashes.pop().unwrap();
                let hash = tree_hash_pair(first, rest);
                hashes.push(hash);
                // cost_before will be lower
                // cost_left is the remaining after computing it
                // the cost of this subtree = after - before
                let used = cost - cost_before;
                cache.insert(original_node, &hash, used);
            }
        }
    }

    assert!(hashes.len() == 1);
    cost += MALLOC_COST_PER_BYTE * 32;
    check_cost(cost, cost_left)?;
    Ok(Reduction(cost, a.new_atom(&hashes[0])?))
}

#[cfg(test)]
mod tests {
    use crate::test_ops::node_eq;

    use super::*;

    // this function neither costs, nor caches
    // and it also returns bytes, rather than an Atom
    pub fn tree_hash(a: &Allocator, node: NodePtr) -> [u8; 32] {
        let mut hashes = Vec::new();
        let mut ops = vec![TreeOp::SExp(node)];

        while let Some(op) = ops.pop() {
            match op {
                TreeOp::SExp(node) => match a.node(node) {
                    NodeVisitor::Buffer(bytes) => {
                        hashes.push(tree_hash_atom(bytes));
                    }
                    NodeVisitor::U32(val) => {
                        if (val as usize) < PRECOMPUTED_HASHES.len() {
                            hashes.push(PRECOMPUTED_HASHES[val as usize]);
                        } else {
                            hashes.push(tree_hash_atom(a.atom(node).as_ref()));
                        }
                    }
                    NodeVisitor::Pair(left, right) => {
                        ops.push(TreeOp::Cons);
                        ops.push(TreeOp::SExp(left));
                        ops.push(TreeOp::SExp(right));
                    }
                },
                TreeOp::Cons => {
                    let first = hashes.pop().unwrap();
                    let rest = hashes.pop().unwrap();
                    hashes.push(tree_hash_pair(first, rest));
                }
                TreeOp::ConsAddCacheCost(_, _) => unreachable!(),
            }
        }

        assert!(hashes.len() == 1);
        hashes[0]
    }

    // this function costs but does not cache
    // we can use it to check that the cache is properly remembering costs
    fn tree_hash_costed(a: &mut Allocator, node: NodePtr, cost_left: Cost) -> Response {
        let mut hashes = Vec::new();
        let mut ops = vec![TreeOp::SExp(node)];

        let mut cost = SHA256TREE_BASE_COST;

        while let Some(op) = ops.pop() {
            match op {
                TreeOp::SExp(node) => {
                    cost += SHA256TREE_COST_PER_CALL;
                    check_cost(cost, cost_left)?;
                    match a.node(node) {
                        NodeVisitor::Buffer(bytes) => {
                            cost += SHA256TREE_COST_PER_BYTE * bytes.len() as u64;
                            check_cost(cost, cost_left)?;

                            let hash = tree_hash_atom(bytes);
                            hashes.push(hash);
                        }
                        NodeVisitor::U32(val) => {
                            cost += SHA256TREE_COST_PER_BYTE * a.atom_len(node) as u64;
                            check_cost(cost, cost_left)?;

                            if (val as usize) < PRECOMPUTED_HASHES.len() {
                                hashes.push(PRECOMPUTED_HASHES[val as usize]);
                            } else {
                                hashes.push(tree_hash_atom(a.atom(node).as_ref()));
                            }
                        }
                        NodeVisitor::Pair(left, right) => {
                            cost += SHA256TREE_COST_PER_BYTE * 65;
                            check_cost(cost, cost_left)?;

                            ops.push(TreeOp::Cons);
                            ops.push(TreeOp::SExp(left));
                            ops.push(TreeOp::SExp(right));
                        }
                    }
                }
                TreeOp::Cons => {
                    let first = hashes.pop().unwrap();
                    let rest = hashes.pop().unwrap();
                    hashes.push(tree_hash_pair(first, rest));
                }
                TreeOp::ConsAddCacheCost(_, _) => unreachable!(),
            }
        }

        assert!(hashes.len() == 1);
        cost += MALLOC_COST_PER_BYTE * 32;
        check_cost(cost, cost_left)?;
        Ok(Reduction(cost, a.new_atom(&hashes[0])?))
    }

    fn test_sha256_atom(buf: &[u8]) {
        let hash = tree_hash_atom(buf);

        let mut hasher = Sha256::new();
        hasher.update([1_u8]);
        if !buf.is_empty() {
            hasher.update(buf);
        }

        assert_eq!(hash.as_ref(), hasher.finalize().as_slice());
    }

    #[test]
    fn test_tree_hash_atom() {
        test_sha256_atom(&[]);
        for val in 0..=255 {
            test_sha256_atom(&[val]);
        }

        for val in 0..=255 {
            test_sha256_atom(&[0, val]);
        }

        for val in 0..=255 {
            test_sha256_atom(&[0xff, val]);
        }
    }

    #[test]
    fn test_precomputed_atoms() {
        assert_eq!(tree_hash_atom(&[]), PRECOMPUTED_HASHES[0]);
        for val in 1..(PRECOMPUTED_HASHES.len() as u8) {
            assert_eq!(tree_hash_atom(&[val]), PRECOMPUTED_HASHES[val as usize]);
        }
    }

    #[test]
    fn test_tree_cache_get() {
        let mut allocator = Allocator::new();
        let mut cache = TreeCache::default();

        let a = allocator.nil();
        let b = allocator.one();
        let c = allocator.new_pair(a, b).expect("new_pair");

        assert_eq!(cache.get(a), None);
        assert_eq!(cache.get(b), None);
        assert_eq!(cache.get(c), None);

        // We don't cache atoms
        cache.insert(a, &tree_hash(&mut allocator, a), 0);
        assert_eq!(cache.get(a), None);

        cache.insert(b, &tree_hash(&mut allocator, b), 0);
        assert_eq!(cache.get(b), None);

        // but pair is OK
        cache.insert(c, &tree_hash(&mut allocator, c), 0);
        let (h, _c) = cache.get(c).expect("expected cached pair");
        assert_eq!(h, &tree_hash(&mut allocator, c));
    }

    #[test]
    fn test_tree_cache_size_limit() {
        let mut allocator = Allocator::new();
        let mut cache = TreeCache::default();

        let mut list = allocator.nil();
        let mut hash = tree_hash(&mut allocator, list);
        cache.insert(list, &hash, 0);

        // we only fit 65k items in the cache
        for i in 0..65540 {
            let b = allocator.one();
            list = allocator.new_pair(b, list).expect("new_pair");
            hash = tree_hash_pair(tree_hash_atom(b"\x01"), hash);
            cache.insert(list, &hash, 0);

            println!("{i}");
            if i < 65533 {
                let (h, _c) = cache.get(list).expect("expected cached");
                assert_eq!(h, &hash);
            } else {
                assert_eq!(cache.get(list), None);
            }
        }
        assert_eq!(cache.get(list), None);
    }

    #[test]
    fn test_tree_cache_should_memoize() {
        let mut allocator = Allocator::new();
        let mut cache = TreeCache::default();

        let a = allocator.nil();
        let b = allocator.one();
        let c = allocator.new_pair(a, b).expect("new_pair");

        assert!(!cache.should_memoize(a));
        assert!(!cache.should_memoize(b));
        assert!(!cache.should_memoize(c));

        // we need to visit a node at least twice for it to be considered a
        // candidate for memoization
        assert!(cache.visit(c));
        assert!(!cache.should_memoize(c));
        assert!(!cache.visit(c));

        assert!(cache.should_memoize(c));
    }

    #[test]
    fn test_tree_hash_costed_equivalence_no_repeats() {
        let mut a = Allocator::new();

        // Build a nested tree:
        // ((a . b) . ((x . y) . (z . w)))
        let a_atom = a.new_atom(b"a").unwrap();
        let b_atom = a.new_atom(b"b").unwrap();
        let x_atom = a.new_atom(b"x").unwrap();
        let y_atom = a.new_atom(b"y").unwrap();
        let z_atom = a.new_atom(b"z").unwrap();
        let w_atom = a.new_atom(b"w").unwrap();

        let ab_pair = a.new_pair(a_atom, b_atom).unwrap();
        let xy_pair = a.new_pair(x_atom, y_atom).unwrap();
        let zw_pair = a.new_pair(z_atom, w_atom).unwrap();
        let right_pair = a.new_pair(xy_pair, zw_pair).unwrap();
        let root = a.new_pair(ab_pair, right_pair).unwrap();

        let cost_left_baseline = 1_000_000;
        let cost_left_cached = 1_000_000;

        // baseline: costed but no caching
        let baseline = tree_hash_costed(&mut a, root, cost_left_baseline).unwrap();

        // cached version
        let mut cache = TreeCache::default();
        cache.visit_tree(&a, root);

        let cached = tree_hash_cached_costed(&mut a, root, &mut cache, cost_left_cached).unwrap();

        assert!(
            !cache.hashes.is_empty(),
            "cache should contain memoized subtrees"
        );

        assert_eq!(
            baseline.0, cached.0,
            "cost mismatch between costed and cached"
        );
        assert!(node_eq(&a, baseline.1, cached.1));

        // the number of cached hashes and costs must match
        assert_eq!(cache.hashes.len(), cache.costs.len());

        // if we re-run with cache, cost_left should still match the baseline
        let cost_left_cached2 = 1_000_000;
        let cached2 = tree_hash_cached_costed(&mut a, root, &mut cache, cost_left_cached2).unwrap();

        assert_eq!(
            cached2.0, cached.0,
            "cost mismatch between costed and cached"
        );
        assert!(node_eq(&a, cached2.1, cached.1));
    }

    #[test]
    fn test_tree_hash_cost_equivalence_with_repeats() {
        let mut a = Allocator::new();
        let x_atom = a.new_atom(b"x").unwrap();
        let y_atom = a.new_atom(b"y").unwrap();
        let r = a.new_pair(x_atom, y_atom).unwrap();

        let left = a.new_pair(r, r).unwrap();
        let right = a.new_pair(r, r).unwrap();
        let root = a.new_pair(left, right).unwrap();

        // Nest it one level deeper:
        let root = a.new_pair(root, root).unwrap();

        let cost_left_baseline = 10_000_000;
        let cost_left_cached = 10_000_000;

        let baseline = tree_hash_costed(&mut a, root, cost_left_baseline).unwrap();

        let mut cache = TreeCache::default();
        cache.visit_tree(&a, root);

        let cached = tree_hash_cached_costed(&mut a, root, &mut cache, cost_left_cached).unwrap();

        assert!(
            !cache.hashes.is_empty(),
            "cache should contain memoized subtrees"
        );

        assert_eq!(
            baseline.0, cached.0,
            "cost mismatch between costed and cached"
        );
        assert!(node_eq(&a, baseline.1, cached.1));

        // run again with same cache — should still match cost and hash
        let cost_left_cached2 = 10_000_000;
        let cached2 = tree_hash_cached_costed(&mut a, root, &mut cache, cost_left_cached2).unwrap();

        assert_eq!(
            cached2.0, cached.0,
            "cost mismatch between costed and cached"
        );
        assert!(node_eq(&a, cached2.1, cached.1));
    }
}
