use crate::allocator::{Allocator, NodePtr};
use crate::cost::check_cost;
use crate::cost::Cost;
use crate::op_utils::atom;
use crate::op_utils::new_atom_and_cost;
use crate::reduction::Response;
use sha3::{Digest, Keccak256};

const KECCAK256_BASE_COST: Cost = 50;
const KECCAK256_COST_PER_ARG: Cost = 160;
const KECCAK256_COST_PER_BYTE: Cost = 2;

pub fn op_keccak256(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = KECCAK256_BASE_COST;

    let mut byte_count: usize = 0;
    let mut hasher = Keccak256::new();
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += KECCAK256_COST_PER_ARG;
        check_cost(
            a,
            cost + byte_count as Cost * KECCAK256_COST_PER_BYTE,
            max_cost,
        )?;
        let blob = atom(a, arg, "keccak256")?;
        byte_count += blob.as_ref().len();
        hasher.update(blob);
    }
    cost += byte_count as Cost * KECCAK256_COST_PER_BYTE;
    new_atom_and_cost(a, cost, &hasher.finalize())
}
