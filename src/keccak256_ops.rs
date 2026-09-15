use crate::allocator::{Allocator, NodePtr};
use crate::chia_dialect::ClvmFlags;
use crate::cost::Cost;
use crate::cost::check_cost;
use crate::op_utils::atom;
use crate::op_utils::new_atom_and_cost;
use crate::reduction::Response;
use sha3::{Digest, Keccak256};

const KECCAK256_BASE_COST: Cost = 50;
const KECCAK256_COST_PER_ARG: Cost = 160;
const KECCAK256_COST_PER_BYTE: Cost = 2;

const NEW_KECCAK256_BASE_COST: Cost = 2350;
const NEW_KECCAK256_COST_PER_ARG: Cost = 100;
const NEW_KECCAK256_COST_PER_BYTE: Cost = 10;

pub fn op_keccak256(
    a: &mut Allocator,
    mut input: NodePtr,
    max_cost: Cost,
    flags: ClvmFlags,
) -> Response {
    let (base_cost, cost_per_arg, cost_per_byte) = if flags.contains(ClvmFlags::NEW_COST_MODEL) {
        (
            NEW_KECCAK256_BASE_COST,
            NEW_KECCAK256_COST_PER_ARG,
            NEW_KECCAK256_COST_PER_BYTE,
        )
    } else {
        (
            KECCAK256_BASE_COST,
            KECCAK256_COST_PER_ARG,
            KECCAK256_COST_PER_BYTE,
        )
    };

    let mut cost = base_cost;

    let mut hasher = Keccak256::new();
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += cost_per_arg;
        let blob = atom(a, arg, "keccak256")?;
        cost += blob.as_ref().len() as Cost * cost_per_byte;
        check_cost(cost, max_cost)?;
        hasher.update(blob);
    }
    new_atom_and_cost(a, cost, &hasher.finalize())
}
