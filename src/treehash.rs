use crate::allocator::NodeVisitor;
use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::cost::check_cost;
use crate::more_ops::PRECOMPUTED_HASHES;
use crate::op_utils::MALLOC_COST_PER_BYTE;
use crate::reduction::Reduction;
use crate::reduction::Response;
use chia_sha2::Sha256;

// the up-front cost of just making the call to sha256tree
const SHA256TREE_BASE_COST: Cost = 270;

// this cost is applied for every pair. Keep in mind that every atom imply a
// pair
const SHA256TREE_PAIR_COST: Cost = 460;
// this is the cost for every 32 bytes in a sha256 call
// it is set to the same as sha256
const SHA256TREE_COST_PER_BYTE: Cost = 2;

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

// this function costs but does not cache
// we can use it to check that the cache is properly remembering costs
pub fn tree_hash_costed(a: &mut Allocator, node: NodePtr, cost_remaining: Cost) -> Response {
    let mut hashes = Vec::new();
    let mut ops = vec![TreeOp::SExp(node)];

    let mut cost = SHA256TREE_BASE_COST;

    while let Some(op) = ops.pop() {
        match op {
            TreeOp::SExp(node) => {
                match a.node(node) {
                    NodeVisitor::Buffer(bytes) => {
                        // +1 byte to length because of prefix before atoms
                        cost += (bytes.len() + 1) as u64 * SHA256TREE_COST_PER_BYTE;
                        check_cost(cost, cost_remaining)?;
                        let hash = tree_hash_atom(bytes);
                        hashes.push(hash);
                    }
                    NodeVisitor::U32(val) => {
                        // This is the case for atoms subject to the small value
                        // optimization. The atom value is stored directly in
                        // the NodePtr, and not on the heap.                        // +1 byte to length because of prefix before atoms
                        cost += (a.atom_len(node) + 1) as u64 * SHA256TREE_COST_PER_BYTE;
                        check_cost(cost, cost_remaining)?;
                        if (val as usize) < PRECOMPUTED_HASHES.len() {
                            //  In this case we save time by not needing to
                            //  allocate a buffer to store the atom in.
                            hashes.push(PRECOMPUTED_HASHES[val as usize]);
                        } else {
                            hashes.push(tree_hash_atom(a.atom(node).as_ref()));
                        }
                    }
                    NodeVisitor::Pair(left, right) => {
                        cost += SHA256TREE_PAIR_COST;
                        check_cost(cost, cost_remaining)?;
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
    check_cost(cost, cost_remaining)?;
    Ok(Reduction(cost, a.new_atom(&hashes[0])?))
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
