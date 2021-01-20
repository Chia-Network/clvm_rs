use bls12_381::{G1Affine, G1Projective, Scalar};
use num_bigint::{BigUint, Sign};
use std::convert::TryFrom;
use std::ops::BitAndAssign;
use std::ops::BitOrAssign;
use std::ops::BitXorAssign;

use lazy_static::lazy_static;

use crate::allocator::Allocator;
use crate::node::Node;
use crate::number::{node_from_number, number_from_u8, Number};
use crate::op_utils::{atom, check_arg_count, int_atom, two_ints, uint_int};
use crate::reduction::{Reduction, Response};
use crate::serialize::node_to_bytes;

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

const DIV_BASE_COST: u32 = 29;
const DIV_COST_PER_LIMB_DIVIDER: u32 = 64;

const SHA256_BASE_COST: u32 = 3;
const SHA256_COST_PER_ARG: u32 = 8;
const SHA256_COST_PER_BYTE_DIVIDER: u32 = 64;

const SHIFT_BASE_COST: u32 = 21;
const SHIFT_COST_PER_BYTE_DIVIDER: u32 = 256;

const BOOL_BASE_COST: u32 = 1;
const BOOL_COST_PER_ARG: u32 = 8;

const POINT_ADD_BASE_COST: u32 = 213;
const POINT_ADD_COST_PER_ARG: u32 = 358;

const PUBKEY_BASE_COST: u32 = 394;
const PUBKEY_COST_PER_BYTE_DIVIDER: u32 = 4;

fn limbs_for_int(v: &Number) -> u32 {
    ((v.bits() + 7) >> 3) as u32
}

pub fn op_sha256<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let mut cost: u32 = SHA256_BASE_COST;
    let mut byte_count: u32 = 0;
    let mut hasher = Sha256::new();
    for arg in args {
        cost += SHA256_COST_PER_ARG;
        let blob = atom(&arg, "sha256")?;
        byte_count += blob.len() as u32;
        hasher.input(blob);
    }
    cost += byte_count / SHA256_COST_PER_BYTE_DIVIDER;
    Ok(Reduction(cost, args.new_atom(&hasher.result()).node))
}

pub fn op_add<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let mut cost: u32 = ARITH_BASE_COST;
    let mut byte_count: u32 = 0;
    let mut total: Number = 0.into();
    for arg in args {
        cost += ARITH_COST_PER_ARG;
        let blob = int_atom(&arg, "+")?;
        let v: Number = number_from_u8(&blob);
        byte_count += blob.len() as u32;
        total += v;
    }
    let total: Node<T> = node_from_number(&args, &total);
    cost += byte_count / ARITH_COST_PER_LIMB_DIVIDER;
    Ok(Reduction(cost, total.node))
}

pub fn op_subtract<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let mut cost: u32 = ARITH_BASE_COST;
    let mut byte_count: u32 = 0;
    let mut total: Number = 0.into();
    let mut is_first = true;
    for arg in args {
        cost += ARITH_COST_PER_ARG;
        let blob = int_atom(&arg, "-")?;
        let v: Number = number_from_u8(&blob);
        byte_count += blob.len() as u32;
        if is_first {
            total += v;
        } else {
            total -= v;
        };
        is_first = false;
    }
    let total: Node<T> = node_from_number(&args, &total);
    cost += byte_count / ARITH_COST_PER_LIMB_DIVIDER;
    Ok(Reduction(cost, total.node))
}

pub fn op_multiply<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let mut cost: u32 = MUL_BASE_COST;
    let mut first_iter: bool = true;
    let mut total: Number = 1.into();
    let mut l0: u32 = 0;
    for arg in args {
        let blob = int_atom(&arg, "*")?;
        if first_iter {
            l0 = blob.len() as u32;
            total = number_from_u8(&blob);
            first_iter = false;
            continue;
        }
        let l1 = blob.len() as u32;

        total *= number_from_u8(&blob);
        cost += MUL_COST_PER_OP;

        cost += (l0 + l1) / MUL_LINEAR_COST_PER_BYTE_DIVIDER;
        cost += (l0 * l1) / MUL_SQUARE_COST_PER_BYTE_DIVIDER;

        l0 = limbs_for_int(&total);
    }
    let total: Node<T> = node_from_number(&args, &total);
    Ok(Reduction(cost, total.node))
}

pub fn op_div<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let (a0, l0, a1, l1) = two_ints(args, "div")?;
    let cost = DIV_BASE_COST + (l0 + l1) / DIV_COST_PER_LIMB_DIVIDER;
    if a1.sign() == Sign::NoSign {
        args.first()?.err("div with 0")
    } else {
        let q = &a0 / &a1;
        let r = &a0 - &a1 * &q;

        // rust rounds division towards zero, but we want division to round
        // toward negative infinity.
        let q = if q.sign() == Sign::Minus && r.sign() != Sign::NoSign {
            q - 1
        } else {
            q
        };
        let q1: Node<T> = node_from_number(&args, &q);
        Ok(Reduction(cost, q1.node))
    }
}

pub fn op_divmod<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let (a0, l0, a1, l1) = two_ints(args, "divmod")?;
    let cost = DIVMOD_BASE_COST + (l0 + l1) / DIVMOD_COST_PER_LIMB_DIVIDER;
    if a1.sign() == Sign::NoSign {
        args.first()?.err("divmod with 0")
    } else {
        let q = &a0 / &a1;
        let r = &a0 - &a1 * &q;

        // rust rounds division towards zero, but we want division to round
        // toward negative infinity.
        let (q, r) = if q.sign() == Sign::Minus && r.sign() != Sign::NoSign {
            (q - 1, r + &a1)
        } else {
            (q, r)
        };
        let q1: Node<T> = node_from_number(&args, &q);
        let r1: Node<T> = node_from_number(&args, &r);
        Ok(Reduction(cost, q1.cons(&r1).node))
    }
}

pub fn op_gr<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(&args, 2, ">")?;
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    let v0 = int_atom(&a0, ">")?;
    let v1 = int_atom(&a1, ">")?;
    let cost = GR_BASE_COST + (v0.len() + v1.len()) as u32 / GR_COST_PER_LIMB_DIVIDER;
    Ok(Reduction(
        cost,
        if number_from_u8(v0) > number_from_u8(v1) {
            args.one().node
        } else {
            args.null().node
        },
    ))
}

pub fn op_gr_bytes<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(&args, 2, ">s")?;
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    let v0 = atom(&a0, ">s")?;
    let v1 = atom(&a1, ">s")?;
    let cost = CMP_BASE_COST + (v0.len() + v1.len()) as u32 / CMP_COST_PER_LIMB_DIVIDER;
    Ok(Reduction(
        cost,
        if v0 > v1 {
            args.one().node
        } else {
            args.null().node
        },
    ))
}

pub fn op_strlen<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(&args, 1, "strlen")?;
    let a0 = args.first()?;
    let v0 = atom(&a0, "strlen")?;
    let size: u32 = v0.len() as u32;
    let size_num: Number = size.into();
    let size_node = node_from_number(&args, &size_num).node;
    let cost: u32 = STRLEN_BASE_COST + size / STRLEN_COST_PER_BYTE_DIVIDER;
    Ok(Reduction(cost, size_node))
}

pub fn op_substr<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(&args, 3, "substr")?;
    let a0 = args.first()?;
    let s0 = atom(&a0, "substr")?;
    let (n1, _, n2, _) = two_ints(&args.rest()?, "substr")?;
    let i1: isize = isize::try_from(n1).unwrap_or(isize::max_value());
    let i2: isize = isize::try_from(n2).unwrap_or(0);
    let size = s0.len() as isize;
    if i2 > size || i2 < i1 || i2 < 0 || i1 < 0 {
        args.err("invalid indices for substr")
    } else {
        let u1: usize = i1 as usize;
        let u2: usize = i2 as usize;
        let r = args.new_atom(&s0[u1..u2]).node;
        let cost = 1;
        Ok(Reduction(cost, r))
    }
}

pub fn op_concat<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let mut cost: u32 = CONCAT_BASE_COST;
    let mut total_size: usize = 0;
    for arg in args {
        cost += CONCAT_COST_PER_ARG;
        let blob = atom(&arg, "concat")?;
        total_size += blob.len();
    }
    let mut v: Vec<u8> = Vec::with_capacity(total_size);

    for arg in args {
        let blob = arg.atom().unwrap();
        v.extend_from_slice(blob);
    }
    cost += (total_size as u32) / CONCAT_COST_PER_BYTE_DIVIDER;
    let allocator: &T = args.allocator;
    let r: T::Ptr = allocator.new_atom(&v);

    Ok(Reduction(cost, r))
}

pub fn op_ash<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let (i0, l0, i1, _) = two_ints(&args, "ash")?;
    let s1 = i64::try_from(&i1);
    if match s1 {
        Err(_) => true,
        Ok(v) => v.abs() > 65535,
    } {
        return args.rest()?.first()?.err("shift too large");
    }

    let a1 = s1.unwrap();
    let v: Number = if a1 > 0 { i0 << a1 } else { i0 >> -a1 };
    let l1 = limbs_for_int(&v);
    let r: Node<T> = node_from_number(&args, &v);
    let cost = SHIFT_BASE_COST + (l0 + l1) / SHIFT_COST_PER_BYTE_DIVIDER;
    Ok(Reduction(cost, r.node))
}

pub fn op_lsh<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let (i0, l0, i1, _) = uint_int(&args, "lsh")?;
    let s1 = i64::try_from(&i1);
    if match s1 {
        Err(_) => true,
        Ok(v) => v.abs() > 65535,
    } {
        return args.rest()?.first()?.err("shift too large");
    }

    let a1 = s1.unwrap();
    let i0: Number = i0.into();
    let v: Number = if a1 > 0 { i0 << a1 } else { i0 >> -a1 };
    let l1 = limbs_for_int(&v);
    let r: Node<T> = node_from_number(&args, &v);
    let cost = SHIFT_BASE_COST + (l0 + l1) / SHIFT_COST_PER_BYTE_DIVIDER;
    Ok(Reduction(cost, r.node))
}

fn binop_reduction<T: Allocator>(
    op_name: &str,
    initial_value: Number,
    args: &Node<T>,
    op_f: fn(&mut Number, &Number) -> (),
) -> Response<T::Ptr> {
    let mut total = initial_value;
    let mut arg_size = 0;
    let mut cost = LOG_BASE_COST;
    for arg in args {
        let blob = int_atom(&arg, op_name)?;
        let n0 = number_from_u8(blob);
        op_f(&mut total, &n0);
        arg_size += blob.len() as u32;
        cost += LOG_COST_PER_ARG;
    }
    cost += arg_size / LOG_COST_PER_LIMB_DIVIDER;
    let total: Node<T> = node_from_number(&args, &total);
    Ok(Reduction(cost, total.node))
}

fn logand_op<T: Allocator>(a: &mut Number, b: &Number) {
    a.bitand_assign(b);
}

pub fn op_logand<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let v: Number = (-1).into();
    binop_reduction("logand", v, args, logand_op::<T>)
}

fn logior_op<T: Allocator>(a: &mut Number, b: &Number) {
    a.bitor_assign(b);
}

pub fn op_logior<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let v: Number = (0).into();
    binop_reduction("logior", v, args, logior_op::<T>)
}

fn logxor_op<T: Allocator>(a: &mut Number, b: &Number) {
    a.bitxor_assign(b);
}

pub fn op_logxor<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let v: Number = (0).into();
    binop_reduction("logxor", v, args, logxor_op::<T>)
}

pub fn op_lognot<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(&args, 1, "lognot")?;
    let a0 = args.first()?;
    let v0 = int_atom(&a0, "lognot")?;
    let mut n: Number = number_from_u8(&v0);
    n = !n;
    let cost: u32 = LOGNOT_BASE_COST + (v0.len() as u32) / LOGNOT_COST_PER_BYTE_DIVIDER;
    let r: Node<T> = node_from_number(&args, &n);
    Ok(Reduction(cost, r.node))
}

pub fn op_not<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(&args, 1, "not")?;
    let r: T::Ptr = args.from_bool(!args.first()?.as_bool()).node;
    let cost: u32 = BOOL_BASE_COST + BOOL_COST_PER_ARG;
    Ok(Reduction(cost, r))
}

pub fn op_any<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let mut cost: u32 = BOOL_BASE_COST;
    let mut is_any = false;
    for arg in args {
        cost += BOOL_COST_PER_ARG;
        is_any = is_any || arg.as_bool();
    }
    let total: Node<T> = args.from_bool(is_any);
    Ok(Reduction(cost, total.node))
}

pub fn op_all<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let mut cost: u32 = BOOL_BASE_COST;
    let mut is_all = true;
    for arg in args {
        cost += BOOL_COST_PER_ARG;
        is_all = is_all && arg.as_bool();
    }
    let total: Node<T> = args.from_bool(is_all);
    Ok(Reduction(cost, total.node))
}

pub fn op_softfork<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    match args.pair() {
        Some((p1, _)) => {
            let n: Number = number_from_u8(int_atom(&p1, "softfork")?);
            if n.sign() == Sign::Plus {
                let cost: u32 = TryFrom::try_from(&n).unwrap_or(u32::max_value());
                Ok(Reduction(cost, args.null().node))
            } else {
                args.err("cost must be > 0")
            }
        }
        _ => args.err("softfork takes at least 1 argument"),
    }
}

lazy_static! {
    static ref GROUP_ORDER: Number = {
        let order_as_hex = b"73EDA753299D7D483339D80809A1D80553BDA402FFFE5BFEFFFFFFFF00000001";
        let n = BigUint::parse_bytes(order_as_hex, 16).unwrap();
        n.into()
    };
}

fn mod_group_order(n: Number) -> Number {
    let order = GROUP_ORDER.clone();
    let divisor: Number = &n / &order;
    let remainder: Number = &n - &divisor * &order;
    if remainder.sign() == Sign::Minus {
        order + remainder
    } else {
        remainder
    }
}

fn number_to_scalar(n: Number) -> Scalar {
    let (sign, as_u8): (Sign, Vec<u8>) = n.to_bytes_le();
    let mut scalar_array: [u8; 32] = [0; 32];
    scalar_array[..as_u8.len()].clone_from_slice(&as_u8[..]);
    let exp: Scalar = Scalar::from_bytes(&scalar_array).unwrap();
    if sign == Sign::Minus {
        exp.neg()
    } else {
        exp
    }
}

pub fn op_pubkey_for_exp<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    check_arg_count(&args, 1, "pubkey_for_exp")?;
    let a0 = args.first()?;

    let v0 = int_atom(&a0, "pubkey_for_exp")?;
    let exp: Number = mod_group_order(number_from_u8(&v0));
    let cost: u32 = PUBKEY_BASE_COST + (v0.len() as u32) / PUBKEY_COST_PER_BYTE_DIVIDER;
    let exp: Scalar = number_to_scalar(exp);
    let point: G1Projective = G1Affine::generator() * exp;
    let point: G1Affine = point.into();
    let point_node: Node<T> = args.new_atom(&point.to_compressed());

    Ok(Reduction(cost, point_node.node))
}

pub fn op_point_add<T: Allocator>(args: &Node<T>) -> Response<T::Ptr> {
    let mut cost: u32 = POINT_ADD_BASE_COST;
    let mut total: G1Projective = G1Projective::identity();
    for arg in args {
        let blob = atom(&arg, "point_add")?;
        let mut is_ok: bool = blob.len() == 48;
        if is_ok {
            let mut as_array: [u8; 48] = [0; 48];
            as_array.clone_from_slice(&blob[0..48]);
            let v = G1Affine::from_compressed(&as_array);
            is_ok = v.is_some().into();
            if is_ok {
                let point = v.unwrap();
                cost += POINT_ADD_COST_PER_ARG;
                total += &point;
            }
        }
        if !is_ok {
            let blob: String = hex::encode(node_to_bytes(&arg).unwrap());
            let msg = format!("point_add expects blob, got {}: Length of bytes object not equal to G1Element::SIZE", blob);
            return args.err(&msg);
        }
    }
    let total: G1Affine = total.into();
    let total: Node<T> = args.new_atom(&total.to_compressed());
    Ok(Reduction(cost, total.node))
}
