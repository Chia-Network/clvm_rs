use crate::allocator::NodeVisitor;
use crate::allocator::{Allocator, NodePtr};
use crate::cost::check_cost;
use crate::cost::Cost;
use crate::more_ops::PRECOMPUTED_HASHES;
use crate::op_utils::MALLOC_COST_PER_BYTE;
use crate::reduction::Reduction;
use crate::reduction::Response;
use chia_sha2::Sha256;

// the base cost is the cost of calling it to begin with
const SHA256TREE_BASE_COST: Cost = 0;
// this is the cost per node, whether it is a cons box or an atom
const SHA256TREE_COST_PER_NODE: Cost = 0;
// this is the cost for every 32 bytes in a sha256 call
const SHA256TREE_COST_PER_32_BYTES: Cost = 700;

pub fn tree_hash_atom(bytes: &[u8]) -> [u8; 32] {
    let mut sha256 = Sha256::new();
    sha256.update([1]);
    sha256.update(bytes);
    sha256.finalize()
}

pub fn tree_hash_pair(first: &[u8; 32], rest: &[u8; 32]) -> [u8; 32] {
    let mut sha256 = Sha256::new();
    sha256.update([2]);
    sha256.update(first);
    sha256.update(rest);
    sha256.finalize()
}

enum TreeOp {
    SExp(NodePtr),
    Cons,
}

// costing is done for every 32 byte chunk that is hashed
#[inline]
fn increment_cost_for_hash_of_bytes(size: usize, cost: &mut Cost) {
    *cost += (size.div_ceil(32)) as u64 * SHA256TREE_COST_PER_32_BYTES;
}

// this function costs but does not cache
// we can use it to check that the cache is properly remembering costs
pub fn tree_hash_costed(a: &mut Allocator, node: NodePtr, cost_left: Cost) -> Response {
    let mut hashes = Vec::new();
    let mut ops = vec![TreeOp::SExp(node)];

    let mut cost = SHA256TREE_BASE_COST;

    while let Some(op) = ops.pop() {
        match op {
            TreeOp::SExp(node) => {
                cost += SHA256TREE_COST_PER_NODE;
                check_cost(cost, cost_left)?;
                match a.node(node) {
                    NodeVisitor::Buffer(bytes) => {
                        increment_cost_for_hash_of_bytes(bytes.len() + 1, &mut cost);
                        check_cost(cost, cost_left)?;
                        let hash = tree_hash_atom(bytes);
                        hashes.push(hash);
                    }
                    NodeVisitor::U32(val) => {
                        increment_cost_for_hash_of_bytes(a.atom_len(node) + 1, &mut cost);
                        check_cost(cost, cost_left)?;
                        if (val as usize) < PRECOMPUTED_HASHES.len() {
                            hashes.push(PRECOMPUTED_HASHES[val as usize]);
                        } else {
                            hashes.push(tree_hash_atom(a.atom(node).as_ref()));
                        }
                    }
                    NodeVisitor::Pair(left, right) => {
                        increment_cost_for_hash_of_bytes(65, &mut cost);
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
                hashes.push(tree_hash_pair(&first, &rest));
            }
        }
    }

    assert!(hashes.len() == 1);
    cost += MALLOC_COST_PER_BYTE * 32;
    check_cost(cost, cost_left)?;
    Ok(Reduction(cost, a.new_atom(&hashes[0])?))
}

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
                hashes.push(tree_hash_pair(&first, &rest));
            }
        }
    }

    assert!(hashes.len() == 1);
    hashes[0]
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
