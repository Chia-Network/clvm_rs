use num_bigint::{BigUint, Sign};
use std::convert::TryFrom;
use std::ops::BitAndAssign;
use std::ops::BitOrAssign;
use std::ops::BitXorAssign;

/*

pub fn op_substr<T>(args: &Node<T>) -> Response<T> {
    if args.arg_count_is(3) {
        let a0 = args.first()?;
        if let Some(v0) = a0.atom() {
            let size: u32 = v0.len() as u32;
            let size_num: Number = size.into();
            let size_node = node_from_number(args.into(), &size_num).node;
            let cost: u32 = STRLEN_BASE_COST + size / STRLEN_COST_PER_BYTE_DIVIDER;
            return Ok(Reduction(cost, size_node));
        } else {
            a0.err("strlen on list")
        }
    } else {
        args.err("substr takes exactly 3 argument")
    }
}




def op_substr(args):
    if args.list_len() != 3:
        raise EvalError("substr takes exactly 3 argument", args)
    a0 = args.first()
    if a0.pair:
        raise EvalError("substr on list", a0)

    i1, i2 = args_as_int_list("substr", args.rest(), 2)

    s0 = a0.as_atom()
    if i2 > len(s0) or i2 < i1 or i2 < 0 or i1 < 0:
        raise EvalError("invalid indices for substr", args)
    s = s0[i1:i2]
    cost = 1
    return cost, args.to(s)



*/

use crate::allocator::Allocator;
use crate::node::Node;
use crate::number::{node_from_number, number_from_u8, Number};
use crate::reduction::{EvalErr, Reduction, Response};

use sha2::{Digest, Sha256};

const ARITH_BASE_COST: u32 = 4;
const ARITH_COST_PER_ARG: u32 = 8;
const ARITH_COST_PER_LIMB_DIVIDER: u32 = 64;

const LOG_BASE_COST: u32 = 6;
const LOG_COST_PER_ARG: u32 = 8;
const LOG_COST_PER_LIMB_DIVIDER: u32 = 64;

const LOGNOT_BASE_COST: u32 = 12;
const LOGNOT_COST_PER_BYTE_DIVIDER: u32 = 512;

const MUL_BASE_COST: u32 = 2;
const MUL_COST_PER_OP: u32 = 18;
const MUL_LINEAR_COST_PER_BYTE_DIVIDER: u32 = 64;
const MUL_SQUARE_COST_PER_BYTE_DIVIDER: u32 = 44500;

const GR_BASE_COST: u32 = 19;
const GR_COST_PER_LIMB_DIVIDER: u32 = 64;

const CMP_BASE_COST: u32 = 16;
const CMP_COST_PER_LIMB_DIVIDER: u32 = 64;

const STRLEN_BASE_COST: u32 = 18;
const STRLEN_COST_PER_BYTE_DIVIDER: u32 = 4096;

const CONCAT_BASE_COST: u32 = 4;
const CONCAT_COST_PER_ARG: u32 = 8;
const CONCAT_COST_PER_BYTE_DIVIDER: u32 = 830;

const DIVMOD_BASE_COST: u32 = 29;
const DIVMOD_COST_PER_LIMB_DIVIDER: u32 = 64;

const SHA256_BASE_COST: u32 = 3;
const SHA256_COST_PER_ARG: u32 = 8;
const SHA256_COST_PER_BYTE_DIVIDER: u32 = 64;

const SHIFT_BASE_COST: u32 = 21;
const SHIFT_COST_PER_BYTE_DIVIDER: u32 = 256;

const BOOL_BASE_COST: u32 = 1;
const BOOL_COST_PER_ARG: u32 = 8;

/*
const POINT_ADD_COST: u32 = 32;
const PUBKEY_FOR_EXP_COST: u32 = 900;

const CONCAT_COST_PER_BYTE: u32 = 2;
const LOGOP_COST_PER_BYTE: u32 = 2;

const BOOL_OP_COST: u32 = 1;
*/

fn limbs_for_int(v: &Number) -> u32 {
    ((v.bits() + 7) >> 3) as u32
}

pub fn two_ints<T>(args: &Node<T>, op_name: &str) -> Result<(Number, Number), EvalErr<T>> {
    if args.arg_count_is(2) {
        let a0 = args.first()?;
        let a1 = args.rest()?.first()?;
        if let Some(v0) = a0.atom() {
            if let Some(v1) = a1.atom() {
                let n0 = number_from_u8(&v0);
                let n1 = number_from_u8(&v1);
                return Ok((n0, n1));
            }
        }
        args.err(&format!("{} requires int args", op_name))
    } else {
        args.err(&format!("{} requires 2 args", op_name))
    }
}

pub fn uint_int<T>(args: &Node<T>, op_name: &str) -> Result<(BigUint, Number), EvalErr<T>> {
    if args.arg_count_is(2) {
        let a0 = args.first()?;
        let a1 = args.rest()?.first()?;
        if let Some(v0) = a0.atom() {
            if let Some(v1) = a1.atom() {
                let n0 = BigUint::from_bytes_be(&v0);
                let n1 = number_from_u8(&v1);
                return Ok((n0, n1));
            }
        }
        args.err(&format!("{} requires int args", op_name))
    } else {
        args.err(&format!("{} requires 2 args", op_name))
    }
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
            None => return args.err("+ requires int args"),
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
            None => return args.err("- requires int args"),
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
            None => return args.err("* requires int args"),
        };
    }
    let total: Node<T> = node_from_number(args.into(), &total);
    Ok(Reduction(cost, total.node))
}

pub fn op_divmod<T>(args: &Node<T>) -> Response<T> {
    let (a0, a1) = two_ints(args, "divmod")?;
    let cost =
        DIVMOD_BASE_COST + (limbs_for_int(&a0) + limbs_for_int(&a1)) / DIVMOD_COST_PER_LIMB_DIVIDER;
    if a1.sign() == Sign::NoSign {
        args.first()?.err("divmod with 0")
    } else {
        let q = &a0 / &a1;
        let r = a0 - a1 * &q;
        let q1: Node<T> = node_from_number(args.into(), &q);
        let r1: Node<T> = node_from_number(args.into(), &r);
        Ok(Reduction(cost, args.from_pair(&q1, &r1).node))
    }
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
        args.err("> requires int args")
    } else {
        args.err("> requires 2 args")
    }
}

pub fn op_gr_bytes<T>(args: &Node<T>) -> Response<T> {
    if args.arg_count_is(2) {
        let a0 = args.first()?;
        let a1 = args.rest()?.first()?;
        let mut cost: u32 = CMP_BASE_COST;
        if let Some(v0) = a0.atom() {
            if let Some(v1) = a1.atom() {
                cost += (v0.len() + v1.len()) as u32 / CMP_COST_PER_LIMB_DIVIDER;
                return Ok(Reduction(
                    cost,
                    if v0 > v1 {
                        args.one().node
                    } else {
                        args.null().node
                    },
                ));
            }
        }
        args.err(">s on list")
    } else {
        args.err(">s requires 2 args")
    }
}

pub fn op_strlen<T>(args: &Node<T>) -> Response<T> {
    if args.arg_count_is(1) {
        let a0 = args.first()?;
        if let Some(v0) = a0.atom() {
            let size: u32 = v0.len() as u32;
            let size_num: Number = size.into();
            let size_node = node_from_number(args.into(), &size_num).node;
            let cost: u32 = STRLEN_BASE_COST + size / STRLEN_COST_PER_BYTE_DIVIDER;
            return Ok(Reduction(cost, size_node));
        } else {
            a0.err("strlen on list")
        }
    } else {
        args.err("strlen takes exactly 1 argument")
    }
}

pub fn op_substr<T>(args: &Node<T>) -> Response<T> {
    if args.arg_count_is(3) {
        let a0 = args.first()?;
        if let Some(s0) = a0.atom() {
            let (n1, n2) = two_ints(&args.rest()?, "substr")?;
            let i1: isize = isize::try_from(n1).unwrap_or(isize::max_value());
            let i2: isize = isize::try_from(n2).unwrap_or(0);
            let size = s0.len() as isize;
            if i2 > size || i2 < i1 || i2 < 0 || i1 < 0 {
                args.err("invalid indices for substr")
            } else {
                let u1: usize = i1 as usize;
                let u2: usize = i2 as usize;
                let r = args.blob_u8(&s0[u1..u2]).node;
                let cost = 1;
                Ok(Reduction(cost, r))
            }
        } else {
            a0.err("substr on list")
        }
    } else {
        args.err("substr takes exactly 3 argument")
    }
}

pub fn op_concat<T>(args: &Node<T>) -> Response<T> {
    let mut cost: u32 = CONCAT_BASE_COST;
    let mut total_size: usize = 0;
    for arg in args {
        cost += CONCAT_COST_PER_ARG;
        match arg.atom() {
            Some(ref blob) => {
                total_size += blob.len();
            }
            None => return arg.err("concat on list"),
        }
    }
    let mut v: Vec<u8> = Vec::with_capacity(total_size);

    for arg in args {
        let blob = arg.atom().unwrap();
        v.extend_from_slice(blob);
    }
    cost += (total_size as u32) / CONCAT_COST_PER_BYTE_DIVIDER;
    let allocator: &dyn Allocator<T> = args.into();
    let r: T = allocator.blob_u8(&v);

    Ok(Reduction(cost, r))
}

pub fn op_ash<T>(args: &Node<T>) -> Response<T> {
    let (i0, i1) = two_ints(&args, "ash")?;
    let s1 = i64::try_from(&i1);
    if {
        match s1 {
            Err(_) => true,
            Ok(v) => v.abs() > 65535,
        }
    } {
        return args.rest()?.first()?.err("shift too large");
    }

    let a1 = s1.unwrap();
    let l1 = limbs_for_int(&i0);
    let v: Number = if a1 > 0 { i0 << a1 } else { i0 >> -a1 };
    let l2 = limbs_for_int(&v);
    let r: Node<T> = node_from_number(args.into(), &v);
    let cost = SHIFT_BASE_COST + (l1 + l2) / SHIFT_COST_PER_BYTE_DIVIDER;
    Ok(Reduction(cost, r.node))
}

pub fn op_lsh<T>(args: &Node<T>) -> Response<T> {
    let (i0, i1) = uint_int(&args, "lsh")?;
    let s1 = i64::try_from(&i1);
    if {
        match s1 {
            Err(_) => true,
            Ok(v) => v.abs() > 65535,
        }
    } {
        return args.rest()?.first()?.err("shift too large");
    }

    let a1 = s1.unwrap();
    let i0: Number = i0.into();
    let l1 = limbs_for_int(&i0);
    let v: Number = if a1 > 0 { i0 << a1 } else { i0 >> -a1 };
    let l2 = limbs_for_int(&v);
    let r: Node<T> = node_from_number(args.into(), &v);
    let cost = SHIFT_BASE_COST + (l1 + l2) / SHIFT_COST_PER_BYTE_DIVIDER;
    Ok(Reduction(cost, r.node))
}

fn binop_reduction<T>(
    op_name: &str,
    initial_value: Number,
    args: &Node<T>,
    op_f: fn(&mut Number, &Number) -> (),
) -> Response<T> {
    let mut total = initial_value;
    let mut arg_size = 0;
    let mut cost = LOG_BASE_COST;
    for arg in args {
        match arg.atom() {
            Some(v0) => {
                let n0 = number_from_u8(&v0);
                op_f(&mut total, &n0);
                arg_size += limbs_for_int(&total);
                cost += LOG_COST_PER_ARG;
            }
            None => {
                return args.err(&format!("{} requires int args", op_name));
            }
        }
    }
    cost += arg_size / LOG_COST_PER_LIMB_DIVIDER;
    let total: Node<T> = node_from_number(args.into(), &total);
    Ok(Reduction(cost, total.node))
}

fn logand_op<T>(a: &mut Number, b: &Number) -> () {
    a.bitand_assign(b);
}

pub fn op_logand<T>(args: &Node<T>) -> Response<T> {
    let v: Number = (-1).into();
    binop_reduction("logand", v, args, logand_op::<T>)
}

fn logior_op<T>(a: &mut Number, b: &Number) -> () {
    a.bitor_assign(b);
}

pub fn op_logior<T>(args: &Node<T>) -> Response<T> {
    let v: Number = (0).into();
    binop_reduction("logior", v, args, logior_op::<T>)
}

fn logxor_op<T>(a: &mut Number, b: &Number) -> () {
    a.bitxor_assign(b);
}

pub fn op_logxor<T>(args: &Node<T>) -> Response<T> {
    let v: Number = (0).into();
    binop_reduction("logxor", v, args, logxor_op::<T>)
}

pub fn op_lognot<T>(args: &Node<T>) -> Response<T> {
    if args.arg_count_is(1) {
        let a0 = args.first()?;
        if let Some(v0) = a0.atom() {
            let mut n: Number = number_from_u8(&v0);
            n = !n;
            let cost: u32 = LOGNOT_BASE_COST + limbs_for_int(&n) / LOGNOT_COST_PER_BYTE_DIVIDER;
            let r: Node<T> = node_from_number(args.into(), &n);
            return Ok(Reduction(cost, r.node));
        } else {
            args.err("lognot requires int args")
        }
    } else {
        args.err("lognot requires 1 arg")
    }
}

pub fn op_not<T>(args: &Node<T>) -> Response<T> {
    if args.arg_count_is(1) {
        let r: T = args.from_bool(!args.first()?.as_bool()).node;
        let cost: u32 = BOOL_BASE_COST + BOOL_COST_PER_ARG;
        Ok(Reduction(cost, r))
    } else {
        args.err("not requires 1 arg")
    }
}

pub fn op_any<T>(args: &Node<T>) -> Response<T> {
    let mut cost: u32 = BOOL_BASE_COST;
    let mut is_any = false;
    for arg in args {
        cost += BOOL_COST_PER_ARG;
        is_any = is_any || arg.as_bool();
    }
    let total: Node<T> = args.from_bool(is_any);
    Ok(Reduction(cost, total.node))
}

pub fn op_all<T>(args: &Node<T>) -> Response<T> {
    let mut cost: u32 = BOOL_BASE_COST;
    let mut is_all = true;
    for arg in args {
        cost += BOOL_COST_PER_ARG;
        is_all = is_all && arg.as_bool();
    }
    let total: Node<T> = args.from_bool(is_all);
    Ok(Reduction(cost, total.node))
}

pub fn op_softfork<T>(args: &Node<T>) -> Response<T> {
    match args.pair() {
        Some((p1, _)) => {
            if let Some(a0) = p1.atom() {
                let n: Number = number_from_u8(&a0);
                if n.sign() == Sign::Plus {
                    let cost: u32 = TryFrom::try_from(&n).unwrap_or(u32::max_value());
                    Ok(Reduction(cost, args.null().node))
                } else {
                    args.err("cost must be > 0")
                }
            } else {
                p1.err("softfork requires an int argument")
            }
        }
        _ => args.err("softfork takes at least 1 argument"),
    }
}
