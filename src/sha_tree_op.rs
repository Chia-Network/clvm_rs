use crate::allocator::{Allocator, Atom, NodePtr};
use crate::cost::{check_cost, Cost};
use crate::error::EvalErr;
use crate::op_utils::{
    atom, first, get_args, get_varargs, int_atom, mod_group_order, new_atom_and_cost, nilp, rest,
    MALLOC_COST_PER_BYTE,
};
use crate::reduction::{Reduction, Response};
use chia_bls::{
    aggregate_pairing, aggregate_verify, hash_to_g1_with_dst, hash_to_g2_with_dst, G1Element,
    G2Element, PublicKey,
};

const SHA256TREE_BASE_COST: Cost = 50;
const SHA256TREE_COST_PER_CALL: Cost = 160;
const SHA256TREE_COST_PER_BYTE: Cost = 2;

pub fn tree_hash_cached_costed(
    a: &Allocator,
    node: NodePtr,
    cache: &mut TreeCache,
    cost_left: &mut u64,
    cost_per_call: Cost,
    cost_per_byte: Cost,
) -> Result<TreeHash, ()> {
    cache.visit_tree(a, node);

    let mut hashes = Vec::new();
    let mut ops = vec![TreeOp::SExp(node)];
    let mut cost = SHA256TREE_BASE_COST;

    // we will call check_cost throughout the runtime so we can exit immediately if we go over cost
    while let Some(op) = ops.pop() {
        cost = cost + SHA256TREE_COST_PER_CALL;
        check_cost(cost, cost_left)?;

        match op {
            TreeOp::SExp(node) => match a.node(node) {
                NodeVisitor::Buffer(bytes) => {
                    cost = cost + (SHA256TREE_COST_PER_BYTE * bytes.len() as u64);
                    check_cost(cost, cost_left)?;
                    let hash = tree_hash_atom(bytes);
                    hashes.push(hash);
                }
                NodeVisitor::U32(val) => {
                    cost = cost + (SHA256TREE_COST_PER_BYTE * a.atom_len(node) as u64);
                    check_cost(cost, cost_left)?;
                    if (val as usize) < PRECOMPUTED_HASHES.len() {
                        hashes.push(PRECOMPUTED_HASHES[val as usize]);
                    } else {
                        hashes.push(tree_hash_atom(a.atom(node).as_ref()));
                    }
                }
                NodeVisitor::Pair(left, right) => {
                    cost = cost + (SHA256TREE_COST_PER_BYTE * 65_u64);
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
    Ok(hashes[0])
}


pub fn op_sha256_tree(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = SHA256TREE_BASE_COST;
    check_cost(cost, max_cost)?;
    let mut total = G1Element::default();
    let mut is_first = true;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        let point = a.g1(arg)?;
        cost += BLS_G1_SUBTRACT_COST_PER_ARG;
        check_cost(cost, max_cost)?;
        if is_first {
            total = point;
        } else {
            total -= &point;
        };
        is_first = false;
    }
    Ok(Reduction(
        cost + 48 * MALLOC_COST_PER_BYTE,
        a.new_g1(total)?,
    ))
}