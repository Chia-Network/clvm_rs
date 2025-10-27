use crate::allocator::NodeVisitor;
use crate::allocator::{Allocator, NodePtr};
use crate::cost::{check_cost, Cost};
use crate::op_utils::get_args;
use crate::reduction::{Reduction, Response};
use crate::treehash::*;

const SHA256TREE_BASE_COST: Cost = 0;
const SHA256TREE_COST_PER_CALL: Cost = 1300;
const SHA256TREE_COST_PER_BYTE: Cost = 10;

pub fn tree_hash_cached_costed(
    a: &mut Allocator,
    node: NodePtr,
    cache: &mut TreeCache,
    cost_left: u64,
) -> Response {
    cache.visit_tree(a, node);

    let mut hashes = Vec::new();
    let mut ops = vec![TreeOp::SExp(node)];
    let mut cost = SHA256TREE_BASE_COST;

    // we will call check_cost throughout the runtime so we can exit immediately if we go over cost
    while let Some(op) = ops.pop() {
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
                    cost += SHA256TREE_COST_PER_BYTE * 65_u64;
                    check_cost(cost, cost_left)?;
                    if let Some(hash) = cache.get(node) {
                        hashes.push(*hash);
                    } else {
                        if cache.should_memoize(node) {
                            ops.push(TreeOp::ConsAddCache(node));
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
            TreeOp::ConsAddCache(original_node) => {
                let first = hashes.pop().unwrap();
                let rest = hashes.pop().unwrap();
                let hash = tree_hash_pair(first, rest);
                hashes.push(hash);
                cache.insert(original_node, &hash);
            }
        }
    }

    assert!(hashes.len() == 1);
    Ok(Reduction(cost, a.new_atom(&hashes[0])?))
}

pub fn op_sha256_tree(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "sha256tree")?;
    let mut cache = TreeCache::default();
    tree_hash_cached_costed(a, n, &mut cache, max_cost)
}
