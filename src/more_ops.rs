use bls12_381::{G1Affine, G1Projective, Scalar};
use num_bigint::{BigUint, Sign};
use num_integer::Integer;
use std::convert::TryFrom;
use std::ops::BitAndAssign;
use std::ops::BitOrAssign;
use std::ops::BitXorAssign;

use lazy_static::lazy_static;

use crate::allocator::{Allocator, NodePtr, SExp};
use crate::cost::{check_cost, Cost};
use crate::err_utils::err;
use crate::node::Node;
use crate::number::{number_from_u8, ptr_from_number, Number};
use crate::op_utils::{
    arg_count, atom, check_arg_count, i32_atom, int_atom, two_ints, u32_from_u8,
};
use crate::reduction::{Reduction, Response};
use crate::serialize::node_to_bytes;
use crate::sha2::{Digest, Sha256};

// We ascribe some additional cost per byte for operations that allocate new atoms
const MALLOC_COST_PER_BYTE: Cost = 10;

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

fn limbs_for_int(v: &Number) -> usize {
    ((v.bits() + 7) / 8) as usize
}

#[cfg(test)]
fn limb_test_helper(bytes: &[u8]) {
    let bigint = Number::from_signed_bytes_be(&bytes);
    println!("{} bits: {}", &bigint, &bigint.bits());

    // redundant leading zeros don't count, since they aren't stored internally
    let expected = if bytes.len() > 0 && bytes[0] == 0 {
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

fn new_atom_and_cost(a: &mut Allocator, cost: Cost, buf: &[u8]) -> Response {
    let c = buf.len() as Cost * MALLOC_COST_PER_BYTE;
    Ok(Reduction(cost + c, a.new_atom(buf)?))
}

fn malloc_cost(a: &Allocator, cost: Cost, ptr: NodePtr) -> Reduction {
    let c = a.atom(ptr).len() as Cost * MALLOC_COST_PER_BYTE;
    Reduction(cost + c, ptr)
}

pub fn op_unknown(
    allocator: &mut Allocator,
    o: NodePtr,
    args: NodePtr,
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
    //                  + 2 bits
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
            let mut cost = ARITH_BASE_COST as u64;
            let mut byte_count: u64 = 0;
            for arg in Node::new(allocator, args) {
                cost += ARITH_COST_PER_ARG as u64;
                let blob = int_atom(&arg, "unknown op")?;
                byte_count += blob.len() as u64;
                check_cost(
                    allocator,
                    cost + (byte_count as Cost * ARITH_COST_PER_BYTE),
                    max_cost,
                )?;
            }
            cost + (byte_count * ARITH_COST_PER_BYTE as u64)
        }
        2 => {
            let mut cost = MUL_BASE_COST as u64;
            let mut first_iter: bool = true;
            let mut l0: u64 = 0;
            for arg in Node::new(allocator, args) {
                let blob = int_atom(&arg, "unknown op")?;
                if first_iter {
                    l0 = blob.len() as u64;
                    first_iter = false;
                    continue;
                }
                let l1 = blob.len() as u64;
                cost += MUL_COST_PER_OP as u64;
                cost += (l0 + l1) * MUL_LINEAR_COST_PER_BYTE as u64;
                cost += (l0 * l1) / MUL_SQUARE_COST_PER_BYTE_DIVIDER as u64;
                l0 += l1;
                check_cost(allocator, cost, max_cost)?;
            }
            cost
        }
        3 => {
            let mut cost = CONCAT_BASE_COST as u64;
            let mut total_size: u64 = 0;
            for arg in Node::new(allocator, args) {
                cost += CONCAT_COST_PER_ARG as u64;
                let blob = atom(&arg, "unknown op")?;
                total_size += blob.len() as u64;
                check_cost(
                    allocator,
                    cost + total_size as Cost * CONCAT_COST_PER_BYTE,
                    max_cost,
                )?;
            }
            cost + total_size * CONCAT_COST_PER_BYTE as u64
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
    assert!(!test_op_unknown(&buf, &mut a, null).is_ok());

    let buf = vec![0xff, 0xff, 0xff];
    assert!(!test_op_unknown(&buf, &mut a, null).is_ok());

    let buf = vec![0xff, 0xff, b'0'];
    assert!(!test_op_unknown(&buf, &mut a, null).is_ok());

    let buf = vec![0xff, 0xff, 0];
    assert!(!test_op_unknown(&buf, &mut a, null).is_ok());

    let buf = vec![0xff, 0xff, 0xcc, 0xcc, 0xfe, 0xed, 0xce];
    assert!(!test_op_unknown(&buf, &mut a, null).is_ok());

    // an empty atom is not a valid opcode
    let buf = Vec::<u8>::new();
    assert!(!test_op_unknown(&buf, &mut a, null).is_ok());

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

pub fn op_sha256(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = SHA256_BASE_COST;
    let mut byte_count: usize = 0;
    let mut hasher = Sha256::new();
    for arg in Node::new(a, input) {
        cost += SHA256_COST_PER_ARG;
        check_cost(
            a,
            cost + byte_count as Cost * SHA256_COST_PER_BYTE,
            max_cost,
        )?;
        let blob = atom(&arg, "sha256")?;
        byte_count += blob.len();
        hasher.update(blob);
    }
    cost += byte_count as Cost * SHA256_COST_PER_BYTE;
    new_atom_and_cost(a, cost, &hasher.finalize())
}

pub fn op_add(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = ARITH_BASE_COST;
    let mut byte_count: usize = 0;
    let mut total: Number = 0.into();
    for arg in Node::new(a, input) {
        cost += ARITH_COST_PER_ARG;
        check_cost(
            a,
            cost + (byte_count as Cost * ARITH_COST_PER_BYTE),
            max_cost,
        )?;
        let blob = int_atom(&arg, "+")?;
        let v: Number = number_from_u8(blob);
        byte_count += blob.len();
        total += v;
    }
    let total = ptr_from_number(a, &total)?;
    cost += byte_count as Cost * ARITH_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, total))
}

pub fn op_subtract(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut cost = ARITH_BASE_COST;
    let mut byte_count: usize = 0;
    let mut total: Number = 0.into();
    let mut is_first = true;
    for arg in Node::new(a, input) {
        cost += ARITH_COST_PER_ARG;
        check_cost(a, cost + byte_count as Cost * ARITH_COST_PER_BYTE, max_cost)?;
        let blob = int_atom(&arg, "-")?;
        let v: Number = number_from_u8(blob);
        byte_count += blob.len();
        if is_first {
            total += v;
        } else {
            total -= v;
        };
        is_first = false;
    }
    let total = ptr_from_number(a, &total)?;
    cost += byte_count as Cost * ARITH_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, total))
}

pub fn op_multiply(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let mut cost: Cost = MUL_BASE_COST;
    let mut first_iter: bool = true;
    let mut total: Number = 1.into();
    let mut l0: usize = 0;
    for arg in Node::new(a, input) {
        check_cost(a, cost, max_cost)?;
        let blob = int_atom(&arg, "*")?;
        if first_iter {
            l0 = blob.len();
            total = number_from_u8(blob);
            first_iter = false;
            continue;
        }
        let l1 = blob.len();

        total *= number_from_u8(blob);
        cost += MUL_COST_PER_OP;

        cost += (l0 + l1) as Cost * MUL_LINEAR_COST_PER_BYTE;
        cost += (l0 * l1) as Cost / MUL_SQUARE_COST_PER_BYTE_DIVIDER;

        l0 = limbs_for_int(&total);
    }
    let total = ptr_from_number(a, &total)?;
    Ok(malloc_cost(a, cost, total))
}

pub fn op_div_impl(a: &mut Allocator, input: NodePtr, mempool: bool) -> Response {
    let args = Node::new(a, input);
    let (a0, l0, a1, l1) = two_ints(&args, "/")?;
    let cost = DIV_BASE_COST + ((l0 + l1) as Cost) * DIV_COST_PER_BYTE;
    if a1.sign() == Sign::NoSign {
        args.first()?.err("div with 0")
    } else {
        if mempool && (a0.sign() == Sign::Minus || a1.sign() == Sign::Minus) {
            return args.err("div operator with negative operands is deprecated");
        }
        let (mut q, r) = a0.div_mod_floor(&a1);

        // this is to preserve a buggy behavior from the initial implementation
        // of this operator.
        if q == (-1).into() && r != 0.into() {
            q += 1;
        }
        let q1 = ptr_from_number(a, &q)?;
        Ok(malloc_cost(a, cost, q1))
    }
}

pub fn op_div(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    op_div_impl(a, input, false)
}

pub fn op_div_deprecated(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    op_div_impl(a, input, true)
}

pub fn op_divmod(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    let (a0, l0, a1, l1) = two_ints(&args, "divmod")?;
    let cost = DIVMOD_BASE_COST + ((l0 + l1) as Cost) * DIVMOD_COST_PER_BYTE;
    if a1.sign() == Sign::NoSign {
        args.first()?.err("divmod with 0")
    } else {
        let (q, r) = a0.div_mod_floor(&a1);
        let q1 = ptr_from_number(a, &q)?;
        let r1 = ptr_from_number(a, &r)?;

        let c = (a.atom(q1).len() + a.atom(r1).len()) as Cost * MALLOC_COST_PER_BYTE;
        let r: NodePtr = a.new_pair(q1, r1)?;
        Ok(Reduction(cost + c, r))
    }
}

pub fn op_gr(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, ">")?;
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    let v0 = int_atom(&a0, ">")?;
    let v1 = int_atom(&a1, ">")?;
    let cost = GR_BASE_COST + (v0.len() + v1.len()) as Cost * GR_COST_PER_BYTE;
    Ok(Reduction(
        cost,
        if number_from_u8(v0) > number_from_u8(v1) {
            a.one()
        } else {
            a.null()
        },
    ))
}

pub fn op_gr_bytes(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, ">s")?;
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    let v0 = atom(&a0, ">s")?;
    let v1 = atom(&a1, ">s")?;
    let cost = GRS_BASE_COST + (v0.len() + v1.len()) as Cost * GRS_COST_PER_BYTE;
    Ok(Reduction(cost, if v0 > v1 { a.one() } else { a.null() }))
}

pub fn op_strlen(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "strlen")?;
    let a0 = args.first()?;
    let v0 = atom(&a0, "strlen")?;
    let size = v0.len();
    let size_num: Number = size.into();
    let size_node = ptr_from_number(a, &size_num)?;
    let cost = STRLEN_BASE_COST + size as Cost * STRLEN_COST_PER_BYTE;
    Ok(malloc_cost(a, cost, size_node))
}

pub fn op_substr(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    let ac = arg_count(&args, 3);
    if !(2..=3).contains(&ac) {
        return args.err("substr takes exactly 2 or 3 arguments");
    }
    let a0 = args.first()?;
    let s0 = atom(&a0, "substr")?;
    let size = s0.len();
    let rest = args.rest()?;
    let i1 = i32_atom(&rest.first()?, "substr")?;
    let rest = rest.rest()?;

    let i2 = if ac == 3 {
        i32_atom(&rest.first()?, "substr")?
    } else {
        size as i32
    };
    if i2 < 0 || i1 < 0 || i2 as usize > size || i2 < i1 {
        args.err("invalid indices for substr")
    } else {
        let atom_node = a0.node;
        let r = a.new_substr(atom_node, i1 as u32, i2 as u32)?;
        let cost: Cost = 1;
        Ok(Reduction(cost, r))
    }
}

pub fn op_concat(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    let mut cost = CONCAT_BASE_COST;
    let mut total_size: usize = 0;
    let mut terms = Vec::<NodePtr>::new();
    for arg in &args {
        cost += CONCAT_COST_PER_ARG;
        check_cost(
            a,
            cost + total_size as Cost * CONCAT_COST_PER_BYTE,
            max_cost,
        )?;
        match arg.sexp() {
            SExp::Pair(_, _) => return arg.err("concat on list"),
            SExp::Atom(b) => total_size += b.len(),
        };
        terms.push(arg.node);
    }

    cost += total_size as Cost * CONCAT_COST_PER_BYTE;
    cost += total_size as Cost * MALLOC_COST_PER_BYTE;
    check_cost(a, cost, max_cost)?;
    let new_atom = a.new_concat(total_size, &terms)?;
    Ok(Reduction(cost, new_atom))
}

pub fn op_ash(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, "ash")?;
    let a0 = args.first()?;
    let b0 = int_atom(&a0, "ash")?;
    let i0 = number_from_u8(b0);
    let l0 = b0.len();
    let rest = args.rest()?;
    let a1 = i32_atom(&rest.first()?, "ash")?;
    if !(-65535..=65535).contains(&a1) {
        return args.rest()?.first()?.err("shift too large");
    }

    let v: Number = if a1 > 0 { i0 << a1 } else { i0 >> -a1 };
    let l1 = limbs_for_int(&v);
    let r = ptr_from_number(a, &v)?;
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
    let args = Node::new(a, input);
    check_arg_count(&args, 2, "lsh")?;
    let a0 = args.first()?;
    let b0 = int_atom(&a0, "lsh")?;
    let i0 = BigUint::from_bytes_be(b0);
    let l0 = b0.len();
    let rest = args.rest()?;
    let a1 = i32_atom(&rest.first()?, "lsh")?;
    if !(-65535..=65535).contains(&a1) {
        return args.rest()?.first()?.err("shift too large");
    }

    let i0: Number = i0.into();

    let v: Number = if a1 > 0 { i0 << a1 } else { i0 >> -a1 };

    let l1 = limbs_for_int(&v);
    let r = ptr_from_number(a, &v)?;
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
    input: NodePtr,
    max_cost: Cost,
    op_f: fn(&mut Number, &Number) -> (),
) -> Response {
    let mut total = initial_value;
    let mut arg_size: usize = 0;
    let mut cost = LOG_BASE_COST;
    for arg in Node::new(a, input) {
        let blob = int_atom(&arg, op_name)?;
        let n0 = number_from_u8(blob);
        op_f(&mut total, &n0);
        arg_size += blob.len();
        cost += LOG_COST_PER_ARG;
        check_cost(a, cost + (arg_size as Cost * LOG_COST_PER_BYTE), max_cost)?;
    }
    cost += arg_size as Cost * LOG_COST_PER_BYTE;
    let total = ptr_from_number(a, &total)?;
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
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "lognot")?;
    let a0 = args.first()?;
    let v0 = int_atom(&a0, "lognot")?;
    let mut n: Number = number_from_u8(v0);
    n = !n;
    let cost = LOGNOT_BASE_COST + ((v0.len() as Cost) * LOGNOT_COST_PER_BYTE);
    let r = ptr_from_number(a, &n)?;
    Ok(malloc_cost(a, cost, r))
}

pub fn op_not(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "not")?;
    let r: NodePtr = args.from_bool(!args.first()?.as_bool()).node;
    let cost = BOOL_BASE_COST;
    Ok(Reduction(cost, r))
}

pub fn op_any(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    let mut cost = BOOL_BASE_COST;
    let mut is_any = false;
    for arg in &args {
        cost += BOOL_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        is_any = is_any || arg.as_bool();
    }
    let total: Node = args.from_bool(is_any);
    Ok(Reduction(cost, total.node))
}

pub fn op_all(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    let mut cost = BOOL_BASE_COST;
    let mut is_all = true;
    for arg in &args {
        cost += BOOL_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        is_all = is_all && arg.as_bool();
    }
    let total: Node = args.from_bool(is_all);
    Ok(Reduction(cost, total.node))
}

pub fn op_softfork(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    match args.pair() {
        Some((p1, _)) => {
            let n: Number = number_from_u8(int_atom(&p1, "softfork")?);
            if n.sign() == Sign::Plus {
                if n > Number::from(max_cost) {
                    return err(a.null(), "cost exceeded");
                }
                let cost: Cost = TryFrom::try_from(&n).unwrap();
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
        let order_as_bytes = &[
            0x73, 0xed, 0xa7, 0x53, 0x29, 0x9d, 0x7d, 0x48, 0x33, 0x39, 0xd8, 0x08, 0x09, 0xa1,
            0xd8, 0x05, 0x53, 0xbd, 0xa4, 0x02, 0xff, 0xfe, 0x5b, 0xfe, 0xff, 0xff, 0xff, 0xff,
            0x00, 0x00, 0x00, 0x01,
        ];
        let n = BigUint::from_bytes_be(order_as_bytes);
        n.into()
    };
}

fn mod_group_order(n: Number) -> Number {
    let order = GROUP_ORDER.clone();
    let mut remainder = n.mod_floor(&order);
    if remainder.sign() == Sign::Minus {
        remainder += order;
    }
    remainder
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

pub fn op_pubkey_for_exp(a: &mut Allocator, input: NodePtr, _max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "pubkey_for_exp")?;
    let a0 = args.first()?;

    let v0 = int_atom(&a0, "pubkey_for_exp")?;
    let exp: Number = mod_group_order(number_from_u8(v0));
    let cost = PUBKEY_BASE_COST + (v0.len() as Cost) * PUBKEY_COST_PER_BYTE;
    let exp: Scalar = number_to_scalar(exp);
    let point: G1Projective = G1Affine::generator() * exp;
    let point: G1Affine = point.into();

    new_atom_and_cost(a, cost, &point.to_compressed())
}

pub fn op_point_add(a: &mut Allocator, input: NodePtr, max_cost: Cost) -> Response {
    let args = Node::new(a, input);
    let mut cost = POINT_ADD_BASE_COST;
    let mut total: G1Projective = G1Projective::identity();
    for arg in &args {
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
                check_cost(a, cost, max_cost)?;
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
    new_atom_and_cost(a, cost, &total.to_compressed())
}
