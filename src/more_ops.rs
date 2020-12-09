use crate::allocator::Allocator;
use crate::node::Node;
use crate::number::{node_from_number, Number};
use crate::types::{EvalErr, Reduction};
use sha2::{Digest, Sha256};
use std::cmp::max;

const MIN_COST: u32 = 10;
const ADD_COST_PER_LIMB: u32 = 10;
const MUL_COST_PER_LIMB: u32 = 10;
// const DIVMOD_COST_PER_LIMB: u32 = 10;

const SHA256_COST: u32 = 10;

/*
const POINT_ADD_COST: u32 = 32;
const PUBKEY_FOR_EXP_COST: u32 = 900;

const CONCAT_COST_PER_BYTE: u32 = 2;
const LOGOP_COST_PER_BYTE: u32 = 2;

const BOOL_OP_COST: u32 = 1;
*/

pub fn limbs_for_int(args: &Node) -> u32 {
    match args.atom() {
        Some(b) => {
            let size = b.len() as u32;
            {
                if size > 0 && b[0] == 0 {
                    size - 1
                } else {
                    size
                }
            }
        }

        _ => 0,
    }
}

pub fn op_sha256(
    allocator: &dyn Allocator<Node>,
    args: &Node,
) -> Result<Reduction<Node>, EvalErr<Node>> {
    let mut cost: u32 = SHA256_COST;
    let mut hasher = Sha256::new();
    for arg in args.clone() {
        match arg.atom() {
            Some(blob) => {
                hasher.input(blob);
                cost += blob.len() as u32;
            }
            None => return allocator.err(&args, "atom expected"),
        }
    }
    Ok(Reduction(cost, allocator.blob_u8(&hasher.result())))
}

pub fn op_add(
    allocator: &dyn Allocator<Node>,
    args: &Node,
) -> Result<Reduction<Node>, EvalErr<Node>> {
    let mut cost: u32 = MIN_COST;
    let mut total: Number = 0.into();
    for arg in args.clone() {
        cost += limbs_for_int(&arg) * ADD_COST_PER_LIMB;
        let v: Option<Number> = Option::from(&arg);
        match v {
            Some(value) => total += value,
            None => return allocator.err(&args, "+ takes integer arguments"),
        }
    }
    let total: Node = node_from_number(allocator, total);
    Ok(Reduction(cost, total))
}

pub fn op_subtract(
    allocator: &dyn Allocator<Node>,
    args: &Node,
) -> Result<Reduction<Node>, EvalErr<Node>> {
    let mut cost: u32 = MIN_COST;
    let mut total: Number = 0.into();
    let mut is_first = true;
    for arg in args.clone() {
        cost += limbs_for_int(&arg) * ADD_COST_PER_LIMB;
        let v: Option<Number> = Option::from(&arg);
        match v {
            Some(value) => {
                if is_first {
                    total += value;
                } else {
                    total -= value;
                };
                is_first = false;
            }
            None => return allocator.err(&args, "- takes integer arguments"),
        }
    }
    let total: Node = node_from_number(allocator, total);
    Ok(Reduction(cost, total))
}

pub fn op_multiply(
    allocator: &dyn Allocator<Node>,
    args: &Node,
) -> Result<Reduction<Node>, EvalErr<Node>> {
    let mut cost: u32 = MIN_COST;
    let mut total: Number = 1.into();
    for arg in args.clone() {
        let total_node: Node = node_from_number(allocator, total);
        cost += MUL_COST_PER_LIMB * limbs_for_int(&arg) * limbs_for_int(&total_node);
        let v: Option<Number> = Option::from(&arg);
        match v {
            Some(value) => total *= value,
            None => return allocator.err(&args, "* takes integer arguments"),
        }
    }
    let total: Node = node_from_number(allocator, total);
    Ok(Reduction(cost, total))
}

pub fn op_gr(
    allocator: &dyn Allocator<Node>,
    args: &Node,
) -> Result<Reduction<Node>, EvalErr<Node>> {
    let a0 = allocator.first(args)?;
    let v0: Option<Number> = Option::from(&a0);
    let a1 = allocator.first(&allocator.rest(args)?)?;
    let v1: Option<Number> = Option::from(&a1);
    let cost = ADD_COST_PER_LIMB * max(limbs_for_int(&a0), limbs_for_int(&a1));
    if let Some(n0) = v0 {
        if let Some(n1) = v1 {
            return Ok(Reduction(
                cost,
                if n0 > n1 {
                    allocator.blob_u8(&[1])
                } else {
                    allocator.null()
                },
            ));
        }
    }
    allocator.err(args, "> on list")
}
