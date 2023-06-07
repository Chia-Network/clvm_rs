use bls12_381::{G1Affine, G1Projective, Scalar};
use num_bigint::{BigUint, Sign};
use num_integer::Integer;
use std::ops::BitAndAssign;
use std::ops::BitOrAssign;
use std::ops::BitXorAssign;

use crate::allocator::{Allocator, NodePtr, SExp};
use crate::cost::{check_cost, Cost};
use crate::err_utils::err;
use crate::number::Number;
use crate::op_utils::{
    atom, atom_len, get_args, get_varargs, i32_atom, int_atom, mod_group_order, new_atom_and_cost,
    nullp, number_to_scalar, u32_from_u8, MALLOC_COST_PER_BYTE,
};
use crate::reduction::{Reduction, Response};
use crate::sha2::{Digest, Sha256};

const ARITH_BASE_COST: Cost = 99;
const ARITH_COST_PER_ARG: Cost = 320;
const ARITH_COST_PER_BYTE: Cost = 3;

const LOG_BASE_COST: Cost = 100;
const LOG_COST_PER_ARG: Cost = 264;
const LOG_COST_PER_BYTE: Cost = 3;

const LOGNOT_BASE_COST: Cost = 331;
const LOGNOT_COST_PER_BYTE: Cost = 3;

const MUL_BASE_COST: Cost = 92;
const MUL_COST_PER_OP: Cost = 885;
const MUL_LINEAR_COST_PER_BYTE: Cost = 6;
const MUL_SQUARE_COST_PER_BYTE_DIVIDER: Cost = 128;

const GR_BASE_COST: Cost = 498;
const GR_COST_PER_BYTE: Cost = 2;

const GRS_BASE_COST: Cost = 117;
const GRS_COST_PER_BYTE: Cost = 1;

const STRLEN_BASE_COST: Cost = 173;
const STRLEN_COST_PER_BYTE: Cost = 1;

const CONCAT_BASE_COST: Cost = 142;
const CONCAT_COST_PER_ARG: Cost = 135;
const CONCAT_COST_PER_BYTE: Cost = 3;

const DIVMOD_BASE_COST: Cost = 1116;
const DIVMOD_COST_PER_BYTE: Cost = 6;

const DIV_BASE_COST: Cost = 988;
const DIV_COST_PER_BYTE: Cost = 4;

const SHA256_BASE_COST: Cost = 87;
const SHA256_COST_PER_ARG: Cost = 134;
const SHA256_COST_PER_BYTE: Cost = 2;

const ASHIFT_BASE_COST: Cost = 596;
const ASHIFT_COST_PER_BYTE: Cost = 3;

const LSHIFT_BASE_COST: Cost = 277;
const LSHIFT_COST_PER_BYTE: Cost = 3;

const BOOL_BASE_COST: Cost = 200;
const BOOL_COST_PER_ARG: Cost = 300;

// Raspberry PI 4 is about 7.679960 / 1.201742 = 6.39 times slower
// in the point_add benchmark

// increased from 31592 to better model Raspberry PI
const POINT_ADD_BASE_COST: Cost = 101094;
// increased from 419994 to better model Raspberry PI
const POINT_ADD_COST_PER_ARG: Cost = 1343980;

// Raspberry PI 4 is about 2.833543 / 0.447859 = 6.32686 times slower
// in the pubkey benchmark

// increased from 419535 to better model Raspberry PI
const PUBKEY_BASE_COST: Cost = 1325730;
// increased from 12 to closer model Raspberry PI
const PUBKEY_COST_PER_BYTE: Cost = 38;

// the new coinid operator
// we subtract 153 cost as a discount, to incentivice using this operator rather
// than "naked" sha256
const COINID_COST: Cost =
    SHA256_BASE_COST + SHA256_COST_PER_ARG * 3 + SHA256_COST_PER_BYTE * (32 + 32 + 8) - 153;

const MODPOW_BASE_COST: Cost = 17000;
const MODPOW_COST_PER_BYTE_BASE_VALUE: Cost = 38;
// the cost for exponent and modular scale by the square of the size of the
// respective operands
const MODPOW_COST_PER_BYTE_EXPONENT: Cost = 3;
const MODPOW_COST_PER_BYTE_MOD: Cost = 21;

fn limbs_for_int(v: &Number) -> usize {
    ((v.bits() + 7) / 8) as usize
}

#[cfg(test)]
fn limb_test_helper(bytes: &[u8]) {
    let bigint = Number::from_signed_bytes_be(bytes);
    println!("{} bits: {}", &bigint, &bigint.bits());

    // redundant leading zeros don't count, since they aren't stored internally
    let expected = if !bytes.is_empty() && bytes[0] == 0 {
        bytes.len() - 1
    } else {
        bytes.len()
    };
    assert_eq!(limbs_for_int(&bigint), expected);
}

#[test]
fn test_limbs_for_int() {
    limb_test_helper(&[]);
    limb_test_helper(&[0x1]);
    limb_test_helper(&[0x80]);
    limb_test_helper(&[0x81]);
    limb_test_helper(&[0x7f]);
    limb_test_helper(&[0xff]);
    limb_test_helper(&[0, 0xff]);
    limb_test_helper(&[0x7f, 0xff]);
    limb_test_helper(&[0x7f, 0]);
    limb_test_helper(&[0x7f, 0x77]);

    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0, 0]);

    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x40, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x20, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x10, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x08, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x04, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x02, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x01, 0, 0, 0, 0, 0, 0]);

    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0, 0, 0]);
    limb_test_helper(&[0x80, 0, 0, 0, 0, 0, 0, 0]);
}

fn malloc_cost(a: &Allocator, cost: Cost, ptr: NodePtr) -> Reduction {
    let c = a.atom_len(ptr) as Cost * MALLOC_COST_PER_BYTE;
    Reduction(cost + c, ptr)
}

pub fn op_unknown(
    allocator: &mut Allocator,
    o: NodePtr,
    mut args: NodePtr,
    max_cost: Cost,
) -> Response {
    // unknown opcode in lenient mode
    // unknown ops are reserved if they start with 0xffff
    // otherwise, unknown ops are no-ops, but they have costs. The cost is computed
    // like this:

    // byte index (reverse):
    // | 4 | 3 | 2 | 1 | 0          |
    // +---+---+---+---+------------+
    // | multiplier    |XX | XXXXXX |
    // +---+---+---+---+---+--------+
    //  ^               ^    ^
    //  |               |    + 6 bits ignored when computing cost
    // cost_multiplier  |
    // (up to 4 bytes)  + 2 bits
    //                    cost_function

    // 1 is always added to the multiplier before using it to multiply the cost, this
    // is since cost may not be 0.

    // cost_function is 2 bits and defines how cost is computed based on arguments:
    // 0: constant, cost is 1 * (multiplier + 1)
    // 1: computed like operator add, multiplied by (multiplier + 1)
    // 2: computed like operator mul, multiplied by (multiplier + 1)
    // 3: computed like operator concat, multiplied by (multiplier + 1)

    // this means that unknown ops where cost_function is 1, 2, or 3, may still be
    // fatal errors if the arguments passed are not atoms.

    let op = allocator.atom(o);

    if op.is_empty() || (op.len() >= 2 && op[0] == 0xff && op[1] == 0xff) {
        return err(o, "reserved operator");
    }

    let cost_function = (op[op.len() - 1] & 0b11000000) >> 6;
    let cost_multiplier: u64 = match u32_from_u8(&op[0..op.len() - 1]) {
        Some(v) => v as u64,
        None => {
            return err(o, "invalid operator");
        }
    };

    let mut cost = match cost_function {
        0 => 1,
        1 => {
            let mut cost = ARITH_BASE_COST;
            let mut byte_count: u64 = 0;
            while let Some((arg, rest)) = allocator.next(args) {
                args = rest;
                cost += ARITH_COST_PER_ARG;
                let len = atom_len(allocator, arg, "unknown op")?;
                byte_count += len as u64;
                check_cost(
                    allocator,
                    cost + (byte_count as Cost * ARITH_COST_PER_BYTE),
                    max_cost,
                )?;
            }
            cost + (byte_count * ARITH_COST_PER_BYTE)
        }
        2 => {
            let mut cost = MUL_BASE_COST;
            let mut first_iter: bool = true;
            let mut l0: u64 = 0;
            while let Some((arg, rest)) = allocator.next(args) {
                args = rest;
                let len = atom_len(allocator, arg, "unknown op")?;
                if first_iter {
                    l0 = len as u64;
                    first_iter = false;
                    continue;
                }
                let l1 = len as u64;
                cost += MUL_COST_PER_OP;
                cost += (l0 + l1) * MUL_LINEAR_COST_PER_BYTE;
                cost += (l0 * l1) / MUL_SQUARE_COST_PER_BYTE_DIVIDER;
                l0 += l1;
                check_cost(allocator, cost, max_cost)?;
            }
            cost
        }
        3 => {
            let mut cost = CONCAT_BASE_COST;
            let mut total_size: u64 = 0;
            while let Some((arg, rest)) = allocator.next(args) {
                args = rest;
                cost += CONCAT_COST_PER_ARG;
                let len = atom_len(allocator, arg, "unknown op")?;
                total_size += len as u64;
                check_cost(
                    allocator,
                    cost + total_size as Cost * CONCAT_COST_PER_BYTE,
                    max_cost,
                )?;
            }
            cost + total_size * CONCAT_COST_PER_BYTE
        }
        _ => 1,
    };

    assert!(cost > 0);

    check_cost(allocator, cost, max_cost)?;
    cost *= cost_multiplier + 1;
    if cost > u32::MAX as u64 {
        err(o, "invalid operator")
    } else {
        Ok(Reduction(cost as Cost, allocator.null()))
    }
}

#[cfg(test)]
fn test_op_unknown(buf: &[u8], a: &mut Allocator, n: NodePtr) -> Response {
    let buf = a.new_atom(buf)?;
    op_unknown(a, buf, n, 1000000)
}

#[test]
fn test_unknown_op_reserved() {
    let mut a = Allocator::new();

    // any op starting with ffff is reserved and a hard failure
    let buf = vec![0xff, 0xff];
    let null = a.null();
    assert!(test_op_unknown(&buf, &mut a, null).is_err());

    let buf = vec![0xff, 0xff, 0xff];
    assert!(test_op_unknown(&buf, &mut a, null).is_err());

    let buf = vec![0xff, 0xff, b'0'];
    assert!(test_op_unknown(&buf, &mut a, null).is_err());

    let buf = vec![0xff, 0xff, 0];
    assert!(test_op_unknown(&buf, &mut a, null).is_err());

    let buf = vec![0xff, 0xff, 0xcc, 0xcc, 0xfe, 0xed, 0xce];
    assert!(test_op_unknown(&buf, &mut a, null).is_err());

    // an empty atom is not a valid opcode
    let buf = Vec::<u8>::new();
    assert!(test_op_unknown(&buf, &mut a, null).is_err());

    // a single ff is not sufficient to be treated as a reserved opcode
    let buf = vec![0xff];
    assert_eq!(
        test_op_unknown(&buf, &mut a, null),
        Ok(Reduction(142, null))
    );

    // leading zeros count, so this is not considered an ffff-prefix
    let buf = vec![0x00, 0xff, 0xff, 0x00, 0x00];
    // the cost is 0xffff00 = 16776960 plus the implied 1
    assert_eq!(
        test_op_unknown(&buf, &mut a, null),
        Ok(Reduction(16776961, null))
    );
}

#[test]
fn test_lenient_mode_last_bits() {
    let mut a = crate::allocator::Allocator::new();

    // the last 6 bits are ignored for computing cost
    let buf = vec![0x3c, 0x3f];
    let null = a.null();
    assert_eq!(test_op_unknown(&buf, &mut a, null), Ok(Reduction(61, null)));

    let buf = vec![0x3c, 0x0f];
    assert_eq!(test_op_unknown(&buf, &mut a, null), Ok(Reduction(61, null)));

    let buf = vec![0x3c, 0x00];
    assert_eq!(test_op_unknown(&buf, &mut a, null), Ok(Reduction(61, null)));

    let buf = vec![0x3c, 0x2c];
    assert_eq!(test_op_unknown(&buf, &mut a, null), Ok(Reduction(61, null)));
}

pub fn op_sha256(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = SHA256_BASE_COST;
    let mut byte_count: usize = 0;
    let mut hasher = Sha256::new();
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += SHA256_COST_PER_ARG;
        check_cost(
            a,
            cost + byte_count as Cost * SHA256_COST_PER_BYTE,
            max_cost,
        )?;
        let blob = atom(a, arg, "sha256")?;
        byte_count += blob.len();
        hasher.update(blob);
    }
    cost += byte_count as Cost * SHA256_COST_PER_BYTE;
    new_atom_and_cost(a, cost, &hasher.finalize())
}

pub fn op_add(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = ARITH_BASE_COST;
    let mut byte_count: usize = 0;
    let mut total: Number = 0.into();
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += ARITH_COST_PER_ARG;
        check_cost(
            a,
            cost + (byte_count as Cost * ARITH_COST_PER_BYTE),
            max_cost,
        )?;
        let (v, len) = int_atom(a, arg, "+")?;
        byte_count += len;
        total += v;
    }
    let total = a.new_number(total)?;
    cost += byte_count as Cost * ARITH_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, total))
}

pub fn op_subtract(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = ARITH_BASE_COST;
    let mut byte_count: usize = 0;
    let mut total: Number = 0.into();
    let mut is_first = true;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += ARITH_COST_PER_ARG;
        check_cost(a, cost + byte_count as Cost * ARITH_COST_PER_BYTE, max_cost)?;
        let (v, len) = int_atom(a, arg, "-")?;
        byte_count += len;
        if is_first {
            total += v;
        } else {
            total -= v;
        };
        is_first = false;
    }
    let total = a.new_number(total)?;
    cost += byte_count as Cost * ARITH_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, total))
}

pub fn op_multiply(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost: Cost = MUL_BASE_COST;
    let mut first_iter: bool = true;
    let mut total: Number = 1.into();
    let mut l0: usize = 0;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        check_cost(a, cost, max_cost)?;
        if first_iter {
            (total, l0) = int_atom(a, arg, "*")?;
            first_iter = false;
            continue;
        }

        let (v0, l1) = int_atom(a, arg, "*")?;

        total *= v0;
        cost += MUL_COST_PER_OP;

        cost += (l0 + l1) as Cost * MUL_LINEAR_COST_PER_BYTE;
        cost += (l0 * l1) as Cost / MUL_SQUARE_COST_PER_BYTE_DIVIDER;

        l0 = limbs_for_int(&total);
    }
    let total = a.new_number(total)?;
    Ok(malloc_cost(a, cost, total))
}

pub fn op_div(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [v0, v1] = get_args::<2>(a, input, "/")?;
    let (a0, a0_len) = int_atom(a, v0, "/")?;
    let (a1, a1_len) = int_atom(a, v1, "/")?;
    let cost = DIV_BASE_COST + ((a0_len + a1_len) as Cost) * DIV_COST_PER_BYTE;
    if a1.sign() == Sign::NoSign {
        err(input, "div with 0")
    } else {
        if a0.sign() == Sign::Minus || a1.sign() == Sign::Minus {
            return err(input, "div operator with negative operands is deprecated");
        }
        let q = a0.div_floor(&a1);
        let q = a.new_number(q)?;
        Ok(malloc_cost(a, cost, q))
    }
}

pub fn op_div_fixed(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [v0, v1] = get_args::<2>(a, input, "/")?;
    let (a0, a0_len) = int_atom(a, v0, "/")?;
    let (a1, a1_len) = int_atom(a, v1, "/")?;
    let cost = DIV_BASE_COST + ((a0_len + a1_len) as Cost) * DIV_COST_PER_BYTE;
    if a1.sign() == Sign::NoSign {
        err(input, "div with 0")
    } else {
        let q = a0.div_floor(&a1);
        let q = a.new_number(q)?;
        Ok(malloc_cost(a, cost, q))
    }
}

pub fn op_divmod(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [v0, v1] = get_args::<2>(a, input, "divmod")?;
    let (a0, a0_len) = int_atom(a, v0, "divmod")?;
    let (a1, a1_len) = int_atom(a, v1, "divmod")?;
    let cost = DIVMOD_BASE_COST + ((a0_len + a1_len) as Cost) * DIVMOD_COST_PER_BYTE;
    if a1.sign() == Sign::NoSign {
        err(input, "divmod with 0")
    } else {
        let (q, r) = a0.div_mod_floor(&a1);
        let q1 = a.new_number(q)?;
        let r1 = a.new_number(r)?;

        let c = (a.atom_len(q1) + a.atom_len(r1)) as Cost * MALLOC_COST_PER_BYTE;
        let r: NodePtr = a.new_pair(q1, r1)?;
        Ok(Reduction(cost + c, r))
    }
}

pub fn op_mod(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [v0, v1] = get_args::<2>(a, input, "mod")?;
    let (a0, a0_len) = int_atom(a, v0, "mod")?;
    let (a1, a1_len) = int_atom(a, v1, "mod")?;
    let cost = DIV_BASE_COST + ((a0_len + a1_len) as Cost) * DIV_COST_PER_BYTE;
    if a1.sign() == Sign::NoSign {
        err(input, "mod with 0")
    } else {
        let q = a.new_number(a0.mod_floor(&a1))?;
        let c = a.atom_len(q) as Cost * MALLOC_COST_PER_BYTE;
        Ok(Reduction(cost + c, q))
    }
}

pub fn op_gr(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [v0, v1] = get_args::<2>(a, input, ">")?;
    let (v0, v0_len) = int_atom(a, v0, ">")?;
    let (v1, v1_len) = int_atom(a, v1, ">")?;
    let cost = GR_BASE_COST + (v0_len + v1_len) as Cost * GR_COST_PER_BYTE;
    Ok(Reduction(cost, if v0 > v1 { a.one() } else { a.null() }))
}

pub fn op_gr_bytes(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n0, n1] = get_args::<2>(a, input, ">s")?;
    let v0 = atom(a, n0, ">s")?;
    let v1 = atom(a, n1, ">s")?;
    let cost = GRS_BASE_COST + (v0.len() + v1.len()) as Cost * GRS_COST_PER_BYTE;
    Ok(Reduction(cost, if v0 > v1 { a.one() } else { a.null() }))
}

pub fn op_strlen(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "strlen")?;
    let size = atom_len(a, n, "strlen")?;
    let size_node = a.new_number(size.into())?;
    let cost = STRLEN_BASE_COST + size as Cost * STRLEN_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, size_node))
}

pub fn op_substr(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let ([a0, start, end], argc) = get_varargs::<3>(a, input, "substr")?;
    if !(2..=3).contains(&argc) {
        return err(input, "substr takes exactly 2 or 3 arguments");
    }
    let size = atom_len(a, a0, "substr")?;
    let start = i32_atom(a, start, "substr")?;

    let end = if argc == 3 {
        i32_atom(a, end, "substr")?
    } else {
        size as i32
    };
    if end < 0 || start < 0 || end as usize > size || end < start {
        err(input, "invalid indices for substr")
    } else {
        let r = a.new_substr(a0, start as u32, end as u32)?;
        let cost: Cost = 1;
        Ok(Reduction(cost, r))
    }
}

pub fn op_concat(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = CONCAT_BASE_COST;
    let mut total_size: usize = 0;
    let mut terms = Vec::<NodePtr>::new();
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += CONCAT_COST_PER_ARG;
        check_cost(
            a,
            cost + total_size as Cost * CONCAT_COST_PER_BYTE,
            max_cost,
        )?;
        match a.sexp(arg) {
            SExp::Pair(_, _) => return err(arg, "concat on list"),
            SExp::Atom() => total_size += a.atom_len(arg),
        };
        terms.push(arg);
    }

    cost += total_size as Cost * CONCAT_COST_PER_BYTE;
    cost += total_size as Cost * MALLOC_COST_PER_BYTE;
    check_cost(a, cost, max_cost)?;
    let new_atom = a.new_concat(total_size, &terms)?;
    Ok(Reduction(cost, new_atom))
}

pub fn op_ash(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n0, n1] = get_args::<2>(a, input, "ash")?;
    let (i0, l0) = int_atom(a, n0, "ash")?;
    let a1 = i32_atom(a, n1, "ash")?;
    if !(-65535..=65535).contains(&a1) {
        return err(n1, "shift too large");
    }

    let v: Number = if a1 > 0 { i0 << a1 } else { i0 >> -a1 };
    let l1 = limbs_for_int(&v);
    let r = a.new_number(v)?;
    let cost = ASHIFT_BASE_COST + ((l0 + l1) as Cost) * ASHIFT_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, r))
}

#[cfg(test)]
fn test_shift(
    op: fn(&mut Allocator, NodePtr, Cost) -> Response,
    a: &mut Allocator,
    a1: &[u8],
    a2: &[u8],
) -> Response {
    let args = a.null();
    let a2 = a.new_atom(a2).unwrap();
    let args = a.new_pair(a2, args).unwrap();
    let a1 = a.new_atom(a1).unwrap();
    let args = a.new_pair(a1, args).unwrap();
    op(a, args, 10000000 as Cost)
}

#[test]
fn test_op_ash() {
    let mut a = Allocator::new();

    assert_eq!(
        test_shift(op_ash, &mut a, &[1], &[0x80, 0, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    assert_eq!(
        test_shift(op_ash, &mut a, &[1], &[0x80, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    let node = test_shift(op_ash, &mut a, &[1], &[0x80, 0]).unwrap().1;
    assert_eq!(a.atom(node), &[]);

    assert_eq!(
        test_shift(op_ash, &mut a, &[1], &[0x7f, 0, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    assert_eq!(
        test_shift(op_ash, &mut a, &[1], &[0x7f, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    let node = test_shift(op_ash, &mut a, &[1], &[0x7f, 0]).unwrap().1;
    // the result is 1 followed by 4064 zeroes
    let node = a.atom(node);
    assert_eq!(node[0], 1);
    assert_eq!(node.len(), 4065);
}

pub fn op_lsh(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n0, n1] = get_args::<2>(a, input, "lsh")?;
    let b0 = atom(a, n0, "lsh")?;
    let a1 = i32_atom(a, n1, "lsh")?;
    if !(-65535..=65535).contains(&a1) {
        return err(n1, "shift too large");
    }
    let i0 = BigUint::from_bytes_be(b0);
    let l0 = b0.len();
    let i0: Number = i0.into();

    let v: Number = if a1 > 0 { i0 << a1 } else { i0 >> -a1 };

    let l1 = limbs_for_int(&v);
    let r = a.new_number(v)?;
    let cost = LSHIFT_BASE_COST + ((l0 + l1) as Cost) * LSHIFT_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, r))
}

#[test]
fn test_op_lsh() {
    let mut a = Allocator::new();

    assert_eq!(
        test_shift(op_lsh, &mut a, &[1], &[0x80, 0, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    assert_eq!(
        test_shift(op_lsh, &mut a, &[1], &[0x80, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    let node = test_shift(op_lsh, &mut a, &[1], &[0x80, 0]).unwrap().1;
    assert_eq!(a.atom(node), &[]);

    assert_eq!(
        test_shift(op_lsh, &mut a, &[1], &[0x7f, 0, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    assert_eq!(
        test_shift(op_lsh, &mut a, &[1], &[0x7f, 0, 0])
            .unwrap_err()
            .1,
        "shift too large"
    );

    let node = test_shift(op_lsh, &mut a, &[1], &[0x7f, 0]).unwrap().1;
    // the result is 1 followed by 4064 zeroes
    let node = a.atom(node);
    assert_eq!(node[0], 1);
    assert_eq!(node.len(), 4065);
}

fn binop_reduction(
    op_name: &str,
    a: &mut Allocator,
    initial_value: Number,
    mut input: NodePtr,
    max_cost: Cost,
    op_f: fn(&mut Number, &Number) -> (),
) -> Response {
    let mut total = initial_value;
    let mut arg_size: usize = 0;
    let mut cost = LOG_BASE_COST;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        let (n0, len) = int_atom(a, arg, op_name)?;
        op_f(&mut total, &n0);
        arg_size += len;
        cost += LOG_COST_PER_ARG;
        check_cost(a, cost + (arg_size as Cost * LOG_COST_PER_BYTE), max_cost)?;
    }
    cost += arg_size as Cost * LOG_COST_PER_BYTE;
    let total = a.new_number(total)?;
    Ok(malloc_cost(a, cost, total))
}

fn logand_op(a: &mut Number, b: &Number) {
    a.bitand_assign(b);
}

pub fn op_logand(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let v: Number = (-1).into();
    binop_reduction("logand", a, v, input, max_cost, logand_op)
}

fn logior_op(a: &mut Number, b: &Number) {
    a.bitor_assign(b);
}

pub fn op_logior(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let v: Number = (0).into();
    binop_reduction("logior", a, v, input, max_cost, logior_op)
}

fn logxor_op(a: &mut Number, b: &Number) {
    a.bitxor_assign(b);
}

pub fn op_logxor(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let v: Number = (0).into();
    binop_reduction("logxor", a, v, input, max_cost, logxor_op)
}

pub fn op_lognot(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "lognot")?;
    let (mut n, len) = int_atom(a, n, "lognot")?;
    n = !n;
    let cost = LOGNOT_BASE_COST + ((len as Cost) * LOGNOT_COST_PER_BYTE);
    let r = a.new_number(n)?;
    Ok(malloc_cost(a, cost, r))
}

pub fn op_not(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "not")?;
    let r = if nullp(a, n) { a.one() } else { a.null() };
    let cost = BOOL_BASE_COST;
    Ok(Reduction(cost, r))
}

pub fn op_any(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = BOOL_BASE_COST;
    let mut is_any = false;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += BOOL_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        is_any = is_any || !nullp(a, arg);
    }
    Ok(Reduction(cost, if is_any { a.one() } else { a.null() }))
}

pub fn op_all(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = BOOL_BASE_COST;
    let mut is_all = true;
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        cost += BOOL_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        is_all = is_all && !nullp(a, arg);
    }
    Ok(Reduction(cost, if is_all { a.one() } else { a.null() }))
}

pub fn op_pubkey_for_exp(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [n] = get_args::<1>(a, input, "pubkey_for_exp")?;
    let (v0, v0_len) = int_atom(a, n, "pubkey_for_exp")?;
    let exp: Number = mod_group_order(v0);
    let cost = PUBKEY_BASE_COST + (v0_len as Cost) * PUBKEY_COST_PER_BYTE;
    let exp: Scalar = number_to_scalar(exp);
    let point: G1Projective = G1Affine::generator() * exp;
    Ok(Reduction(
        cost + 48 * MALLOC_COST_PER_BYTE,
        a.new_g1(point)?,
    ))
}

pub fn op_point_add(a: &mut Allocator, mut input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = POINT_ADD_BASE_COST;
    let mut total: G1Projective = G1Projective::identity();
    while let Some((arg, rest)) = a.next(input) {
        input = rest;
        let point = a.g1(arg)?;
        cost += POINT_ADD_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        total += &point;
    }
    Ok(Reduction(
        cost + 48 * MALLOC_COST_PER_BYTE,
        a.new_g1(total)?,
    ))
}

pub fn op_coinid(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let [parent_coin, puzzle_hash, amount] = get_args::<3>(a, input, "coinid")?;

    let parent_coin = atom(a, parent_coin, "coinid")?;
    if parent_coin.len() != 32 {
        return err(input, "coinid: invalid parent coin id (must be 32 bytes)");
    }
    let puzzle_hash = atom(a, puzzle_hash, "coinid")?;
    if puzzle_hash.len() != 32 {
        return err(input, "coinid: invalid puzzle hash (must be 32 bytes)");
    }
    let amount = atom(a, amount, "coinid")?;
    if !amount.is_empty() {
        if (amount[0] & 0x80) != 0 {
            return err(input, "coinid: invalid amount (may not be negative");
        }
        if amount == [0_u8] || (amount.len() > 1 && amount[0] == 0 && (amount[1] & 0x80) == 0) {
            return err(
                input,
                "coinid: invalid amount (may not have redundant leading zero)",
            );
        }
        // the only valid coin value that's 9 bytes is when a leading zero is
        // required to not have the value interpreted as negative
        if amount.len() > 9 || (amount.len() == 9 && amount[0] != 0) {
            return err(
                input,
                "coinid: invalid amount (may not exceed max coin amount)",
            );
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(parent_coin);
    hasher.update(puzzle_hash);
    hasher.update(amount);
    let ret: [u8; 32] = hasher
        .finalize()
        .as_slice()
        .try_into()
        .expect("sha256 hash is not 32 bytes");

    new_atom_and_cost(a, COINID_COST, &ret)
}

pub fn op_modpow(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let [base, exponent, modulus] = get_args::<3>(a, input, "modpow")?;

    let mut cost = MODPOW_BASE_COST;
    let (base, bsize) = int_atom(a, base, "modpow")?;
    cost += bsize as Cost * MODPOW_COST_PER_BYTE_BASE_VALUE;
    let (exponent, esize) = int_atom(a, exponent, "modpow")?;
    cost += (esize * esize) as Cost * MODPOW_COST_PER_BYTE_EXPONENT;
    check_cost(a, cost, max_cost)?;
    let (modulus, msize) = int_atom(a, modulus, "modpow")?;
    cost += (msize * msize) as Cost * MODPOW_COST_PER_BYTE_MOD;
    check_cost(a, cost, max_cost)?;

    if exponent.sign() == Sign::Minus {
        return err(input, "modpow with negative exponent");
    }

    if modulus.sign() == Sign::NoSign {
        return err(input, "modpow with 0 modulus");
    }

    let ret = base.modpow(&exponent, &modulus);
    let ret = a.new_number(ret)?;
    Ok(malloc_cost(a, cost, ret))
}
