use crate::node::Node;
use crate::number::{node_from_number, number_from_u8, Number};
use crate::reduction::{Reduction, Response};

use sha2::{Digest, Sha256};

const ARITH_BASE_COST: u32 = 4;
const ARITH_COST_PER_ARG: u32 = 8;
const ARITH_COST_PER_LIMB_DIVIDER: u32 = 64;

const LOG_BASE_COST: u32 = 6;
const LOG_COST_PER_ARG: u32 = 8;
const LOG_COST_PER_LIMB_DIVIDER: u32 = 64;

const MUL_BASE_COST: u32 = 2;
const MUL_COST_PER_OP: u32 = 18;
const MUL_LINEAR_COST_PER_BYTE_DIVIDER: u32 = 64;
const MUL_SQUARE_COST_PER_BYTE_DIVIDER: u32 = 44500;

const GR_BASE_COST: u32 = 19;
const GR_COST_PER_LIMB_DIVIDER: u32 = 64;

// const DIVMOD_COST_PER_LIMB: u32 = 10;

const SHA256_BASE_COST: u32 = 3;
const SHA256_COST_PER_ARG: u32 = 8;
const SHA256_COST_PER_BYTE_DIVIDER: u32 = 64;

/*
const POINT_ADD_COST: u32 = 32;
const PUBKEY_FOR_EXP_COST: u32 = 900;

const CONCAT_COST_PER_BYTE: u32 = 2;
const LOGOP_COST_PER_BYTE: u32 = 2;

const BOOL_OP_COST: u32 = 1;
*/

pub fn limbs_for_int(v: &Number) -> u32 {
    ((v.bits() + 7) >> 3) as u32
}

pub fn op_sha256<T>(args: &Node<T>) -> Response<T> {
    let mut cost: u32 = SHA256_BASE_COST;
    let mut byte_count: u32 = 0;
    let mut hasher = Sha256::new();
    for arg in args {
        cost += SHA256_COST_PER_ARG;
        match arg.atom() {
            Some(ref blob) => {
                byte_count += blob.len() as u32;
                hasher.input(blob);
            }
            None => return arg.err("sha256 got list"),
        }
    }
    cost += byte_count / SHA256_COST_PER_BYTE_DIVIDER;
    Ok(Reduction(cost, args.blob_u8(&hasher.result()).node))
}

pub fn op_add<T>(args: &Node<T>) -> Response<T> {
    let mut cost: u32 = ARITH_BASE_COST;
    let mut byte_count: u32 = 0;
    let mut total: Number = 0.into();
    for arg in args {
        cost += ARITH_COST_PER_ARG;
        match arg.atom() {
            Some(ref blob) => {
                let v: Number = number_from_u8(&blob);
                byte_count += limbs_for_int(&v);
                total += v;
            }
            None => return args.err("+ takes integer arguments"),
        }
    }
    let total: Node<T> = node_from_number(args.into(), &total);
    cost += byte_count / ARITH_COST_PER_LIMB_DIVIDER;
    Ok(Reduction(cost, total.node))
}

pub fn op_subtract<T>(args: &Node<T>) -> Response<T> {
    let mut cost: u32 = ARITH_BASE_COST;
    let mut byte_count: u32 = 0;
    let mut total: Number = 0.into();
    let mut is_first = true;
    for arg in args {
        cost += ARITH_COST_PER_ARG;
        match arg.atom() {
            Some(ref blob) => {
                let v: Number = number_from_u8(&blob);
                byte_count += blob.len() as u32;
                if is_first {
                    total += v;
                } else {
                    total -= v;
                };
                is_first = false;
            }
            None => return args.err("+ takes integer arguments"),
        }
    }
    let total: Node<T> = node_from_number(args.into(), &total);
    cost += byte_count / ARITH_COST_PER_LIMB_DIVIDER;
    Ok(Reduction(cost, total.node))
}

pub fn op_multiply<T>(args: &Node<T>) -> Response<T> {
    let mut cost: u32 = MUL_BASE_COST;
    let mut first_iter: bool = true;
    let mut total: Number = 1.into();
    for arg in args {
        match arg.atom() {
            Some(ref blob) => {
                if first_iter {
                    total = number_from_u8(&blob);
                    first_iter = false;
                    continue;
                }
                let v: Number = number_from_u8(&blob);
                let rs = limbs_for_int(&total);
                let vs = limbs_for_int(&v);

                total *= v;
                cost += MUL_COST_PER_OP;

                cost += (rs + vs) / MUL_LINEAR_COST_PER_BYTE_DIVIDER;
                cost += (rs * vs) / MUL_SQUARE_COST_PER_BYTE_DIVIDER;
            }
            None => return args.err("* takes integer arguments"),
        };
    }
    let total: Node<T> = node_from_number(args.into(), &total);
    Ok(Reduction(cost, total.node))
}

pub fn op_gr<T>(args: &Node<T>) -> Response<T> {
    if args.arg_count_is(2) {
        let a0 = args.first()?;
        let a1 = args.rest()?.first()?;
        let mut cost: u32 = GR_BASE_COST;
        if let Some(v0) = a0.atom() {
            if let Some(v1) = a1.atom() {
                let n0 = number_from_u8(&v0);
                let n1 = number_from_u8(&v1);
                cost += (limbs_for_int(&n0) + limbs_for_int(&n1)) as u32 / GR_COST_PER_LIMB_DIVIDER;
                return Ok(Reduction(
                    cost,
                    if n0 > n1 {
                        args.one().node
                    } else {
                        args.null().node
                    },
                ));
            }
        }
        args.err("> on list")
    } else {
        args.err("> requires 2 args")
    }
}
