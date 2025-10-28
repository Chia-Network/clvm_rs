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
        // charge a call cost for processing this op
        cost += SHA256TREE_COST_PER_CALL;
        check_cost(cost, cost_left)?;
        match op {
            TreeOp::SExp(node) => match a.node(node) {
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
                    // pair cost (65 bytes as before)
                    cost += SHA256TREE_COST_PER_BYTE * 65_u64;
                    check_cost(cost, cost_left)?;
                    if let Some((hash, cached_cost)) = cache.get(node) {
                        // when reusing a cached subtree, charge its cached cost
                        cost += cached_cost;
                        check_cost(cost, cost_left)?;
                        hashes.push(*hash);
                    } else {
                        if cache.should_memoize(node) {
                            // record the cost_left before traversing this subtree
                            ops.push(TreeOp::ConsAddCacheCost(node, cost));
                        } else {
                            ops.push(TreeOp::Cons);
                        }
                        ops.push(TreeOp::SExp(left));
                        ops.push(TreeOp::SExp(right));
                    }
                }
            },
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
