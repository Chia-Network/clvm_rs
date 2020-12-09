use crate::allocator::NodeT;
use crate::number::{node_from_number, Number};
use crate::reduction::{Reduction, Response};

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

pub fn limbs_for_int<T>(args: &NodeT<T>) -> u32 {
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

pub fn op_sha256<T>(args: &NodeT<T>) -> Response<T> {
    let mut cost: u32 = SHA256_COST;
    let mut hasher = Sha256::new();
    for arg in args {
        match arg.atom() {
            Some(ref blob) => {
                hasher.input(blob);
                cost += blob.len() as u32;
            }
            None => return args.err("atom expected"),
        }
    }
    Ok(Reduction(cost, args.blob_u8(&hasher.result()).node))
}

pub fn op_add<T>(args: &NodeT<T>) -> Response<T> {
    let mut cost: u32 = MIN_COST;
    let mut total: Number = 0.into();
    for arg in args {
        cost += limbs_for_int(&arg) * ADD_COST_PER_LIMB;
        let v: Option<Number> = Option::from(&arg);
        match v {
            Some(value) => total += value,
            None => return args.err("+ takes integer arguments"),
        }
    }
    let total: NodeT<T> = node_from_number(args.allocator, total);
    Ok(Reduction(cost, total.node))
}

pub fn op_subtract<T>(args: &NodeT<T>) -> Response<T> {
    let mut cost: u32 = MIN_COST;
    let mut total: Number = 0.into();
    let mut is_first = true;
    for arg in args {
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
            None => return args.err("- takes integer arguments"),
        }
    }
    let total: NodeT<T> = node_from_number(args.allocator, total);
    Ok(Reduction(cost, total.node))
}

pub fn op_multiply<T>(args: &NodeT<T>) -> Response<T> {
    let mut cost: u32 = MIN_COST;
    let mut total: Number = 1.into();
    for arg in args {
        let total_node: NodeT<T> = node_from_number(args.allocator, total);
        cost += MUL_COST_PER_LIMB * limbs_for_int(&arg) * limbs_for_int(&total_node);
        let v: Option<Number> = Option::from(&arg);
        match v {
            Some(value) => total *= value,
            None => return args.err("* takes integer arguments"),
        }
    }
    let total: NodeT<T> = node_from_number(args.allocator, total);
    Ok(Reduction(cost, total.node))
}

pub fn op_gr<T>(args: &NodeT<T>) -> Response<T> {
    let a0 = args.first()?;
    let v0: Option<Number> = Option::from(&a0);
    let a1 = args.rest()?.first()?;
    let v1: Option<Number> = Option::from(&a1);
    let cost = ADD_COST_PER_LIMB * max(limbs_for_int(&a0), limbs_for_int(&a1));
    if let Some(n0) = v0 {
        if let Some(n1) = v1 {
            return Ok(Reduction(
                cost,
                if n0 > n1 {
                    args.blob_u8(&[1]).node
                } else {
                    args.null()
                },
            ));
        }
    }
    args.err("> on list")
}
