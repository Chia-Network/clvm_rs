use bls12_381::{G1Affine, G1Projective, Scalar};
use num_bigint::{BigUint, Sign};
use std::convert::TryFrom;
use std::ops::BitAndAssign;
use std::ops::BitOrAssign;
use std::ops::BitXorAssign;

use lazy_static::lazy_static;

use crate::allocator::Allocator;
use crate::cost::{check_cost, Cost};
use crate::err_utils::{err, u8_err};
use crate::node::Node;
use crate::number::{number_from_u8, ptr_from_number, Number};
use crate::op_utils::{atom, check_arg_count, i32_atom, int_atom, two_ints, u32_from_u8};
use crate::reduction::{Reduction, Response};
use crate::serialize::node_to_bytes;

use sha2::{Digest, Sha256};

const ARITH_BASE_COST: Cost = 4;
const ARITH_COST_PER_ARG: Cost = 8;
const ARITH_COST_PER_LIMB_DIVIDER: Cost = 64;

const LOG_BASE_COST: Cost = 6;
const LOG_COST_PER_ARG: Cost = 8;
const LOG_COST_PER_LIMB_DIVIDER: Cost = 64;

const LOGNOT_BASE_COST: Cost = 12;
const LOGNOT_COST_PER_BYTE_DIVIDER: Cost = 512;

const MUL_BASE_COST: Cost = 2;
const MUL_COST_PER_OP: Cost = 18;
const MUL_LINEAR_COST_PER_BYTE_DIVIDER: Cost = 64;
const MUL_SQUARE_COST_PER_BYTE_DIVIDER: Cost = 44500;

const GR_BASE_COST: Cost = 19;
const GR_COST_PER_LIMB_DIVIDER: Cost = 64;

const CMP_BASE_COST: Cost = 16;
const CMP_COST_PER_LIMB_DIVIDER: Cost = 64;

const STRLEN_BASE_COST: Cost = 18;
const STRLEN_COST_PER_BYTE_DIVIDER: Cost = 4096;

const CONCAT_BASE_COST: Cost = 4;
const CONCAT_COST_PER_ARG: Cost = 8;
const CONCAT_COST_PER_BYTE_DIVIDER: Cost = 830;

const DIVMOD_BASE_COST: Cost = 29;
const DIVMOD_COST_PER_LIMB_DIVIDER: Cost = 64;

const DIV_BASE_COST: Cost = 29;
const DIV_COST_PER_LIMB_DIVIDER: Cost = 64;

const SHA256_BASE_COST: Cost = 3;
const SHA256_COST_PER_ARG: Cost = 8;
const SHA256_COST_PER_BYTE_DIVIDER: Cost = 64;

const SHIFT_BASE_COST: Cost = 21;
const SHIFT_COST_PER_BYTE_DIVIDER: Cost = 256;

const BOOL_BASE_COST: Cost = 1;
const BOOL_COST_PER_ARG: Cost = 8;

const POINT_ADD_BASE_COST: Cost = 213;
const POINT_ADD_COST_PER_ARG: Cost = 358;

const PUBKEY_BASE_COST: Cost = 394;
const PUBKEY_COST_PER_BYTE_DIVIDER: Cost = 4;

fn limbs_for_int(v: &Number) -> usize {
    ((v.bits() + 7) / 8) as usize
}

pub fn op_unknown<A: Allocator>(
    allocator: &mut A,
    o: A::AtomBuf,
    args: A::Ptr,
    max_cost: Cost,
) -> Response<A::Ptr> {
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

    let op = allocator.buf(&o);

    if op.is_empty() || (op.len() >= 2 && op[0] == 0xff && op[1] == 0xff) {
        return u8_err(allocator, &o, "reserved operator");
    }

    let cost_function = (op[op.len() - 1] & 0b11000000) >> 6;
    let cost_multiplier: u64 = match u32_from_u8(&op[0..op.len() - 1]) {
        Some(v) => v as u64,
        None => {
            return u8_err(allocator, &o, "invalid operator");
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
                check_cost(allocator, cost, max_cost)?;
            }
            cost + byte_count / ARITH_COST_PER_LIMB_DIVIDER as u64
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
                cost += (l0 + l1) / MUL_LINEAR_COST_PER_BYTE_DIVIDER as u64;
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
                check_cost(allocator, cost, max_cost)?;
            }
            cost + total_size / CONCAT_COST_PER_BYTE_DIVIDER as u64
        }
        _ => 1,
    };

    assert!(cost > 0);

    check_cost(allocator, cost, max_cost)?;
    cost *= cost_multiplier + 1;
    if cost > u32::MAX as u64 {
        u8_err(allocator, &o, "invalid operator")
    } else {
        Ok(Reduction(cost as Cost, allocator.null()))
    }
}

#[cfg(test)]
fn test_op_unknown<A: Allocator>(buf: &[u8], a: &mut A, n: A::Ptr) -> Response<A::Ptr> {
    use crate::allocator::SExp;

    let buf = a.new_atom(buf)?;
    let abuf = match a.sexp(&buf) {
        SExp::Atom(abuf) => abuf,
        _ => panic!("shouldn't happen"),
    };
    op_unknown(a, abuf, n, 1000000)
}

#[test]
fn test_unknown_op_reserved() {
    let mut a = crate::int_allocator::IntAllocator::new();

    // any op starting with ffff is reserved and a hard failure
    let buf = vec![0xff, 0xff];
    let null = a.null();
    assert!(!test_op_unknown(&buf, &mut a, null).is_ok());

    let buf = vec![0xff, 0xff, 0xff];
    assert!(!test_op_unknown(&buf, &mut a, null).is_ok());

    let buf = vec![0xff, 0xff, '0' as u8];
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
    assert_eq!(test_op_unknown(&buf, &mut a, null), Ok(Reduction(4, null)));

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
    let mut a = crate::int_allocator::IntAllocator::new();

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

pub fn op_sha256<T: Allocator>(a: &mut T, input: T::Ptr, max_cost: Cost) -> Response<T::Ptr> {
    let mut cost = SHA256_BASE_COST;
    let mut byte_count: usize = 0;
    let mut hasher = Sha256::new();
    for arg in Node::new(a, input) {
        cost += SHA256_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        let blob = atom(&arg, "sha256")?;
        byte_count += blob.len();
        hasher.input(blob);
    }
    cost += byte_count as Cost / SHA256_COST_PER_BYTE_DIVIDER;
    Ok(Reduction(cost, a.new_atom(&hasher.result())?))
}

pub fn op_add<T: Allocator>(a: &mut T, input: T::Ptr, max_cost: Cost) -> Response<T::Ptr> {
    let mut cost = ARITH_BASE_COST;
    let mut byte_count: usize = 0;
    let mut total: Number = 0.into();
    for arg in Node::new(a, input) {
        cost += ARITH_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        let blob = int_atom(&arg, "+")?;
        let v: Number = number_from_u8(&blob);
        byte_count += blob.len();
        total += v;
    }
    let total = ptr_from_number(a, &total)?;
    cost += byte_count as Cost / ARITH_COST_PER_LIMB_DIVIDER;
    Ok(Reduction(cost, total))
}

pub fn op_subtract<T: Allocator>(a: &mut T, input: T::Ptr, max_cost: Cost) -> Response<T::Ptr> {
    let mut cost = ARITH_BASE_COST;
    let mut byte_count: usize = 0;
    let mut total: Number = 0.into();
    let mut is_first = true;
    for arg in Node::new(a, input) {
        cost += ARITH_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        let blob = int_atom(&arg, "-")?;
        let v: Number = number_from_u8(&blob);
        byte_count += blob.len();
        if is_first {
            total += v;
        } else {
            total -= v;
        };
        is_first = false;
    }
    let total = ptr_from_number(a, &total)?;
    cost += byte_count as Cost / ARITH_COST_PER_LIMB_DIVIDER;
    Ok(Reduction(cost, total))
}

pub fn op_multiply<T: Allocator>(a: &mut T, input: T::Ptr, max_cost: Cost) -> Response<T::Ptr> {
    let mut cost: Cost = MUL_BASE_COST;
    let mut first_iter: bool = true;
    let mut total: Number = 1.into();
    let mut l0: usize = 0;
    for arg in Node::new(a, input) {
        check_cost(a, cost, max_cost)?;
        let blob = int_atom(&arg, "*")?;
        if first_iter {
            l0 = blob.len();
            total = number_from_u8(&blob);
            first_iter = false;
            continue;
        }
        let l1 = blob.len();

        total *= number_from_u8(&blob);
        cost += MUL_COST_PER_OP;

        cost += (l0 + l1) as Cost / MUL_LINEAR_COST_PER_BYTE_DIVIDER;
        cost += (l0 * l1) as Cost / MUL_SQUARE_COST_PER_BYTE_DIVIDER;

        l0 = limbs_for_int(&total);
    }
    let total = ptr_from_number(a, &total)?;
    Ok(Reduction(cost, total))
}

pub fn op_div<T: Allocator>(a: &mut T, input: T::Ptr, _max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    let (a0, l0, a1, l1) = two_ints(&args, "/")?;
    let cost = DIV_BASE_COST + ((l0 + l1) as Cost) / DIV_COST_PER_LIMB_DIVIDER;
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
        let q1 = ptr_from_number(a, &q)?;
        Ok(Reduction(cost, q1))
    }
}

pub fn op_divmod<T: Allocator>(a: &mut T, input: T::Ptr, _max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    let (a0, l0, a1, l1) = two_ints(&args, "divmod")?;
    let cost = DIVMOD_BASE_COST + ((l0 + l1) as Cost) / DIVMOD_COST_PER_LIMB_DIVIDER;
    if a1.sign() == Sign::NoSign {
        args.first()?.err("divmod with 0")
    } else {
        let q = &a0 / &a1;
        let r = &a0 - &a1 * &q;

        let signed_quotient =
            (a0.sign() == Sign::Minus || a1.sign() == Sign::Minus) && a0.sign() != a1.sign();

        // rust rounds division towards zero, but we want division to round
        // toward negative infinity.
        let (q, r) = if signed_quotient && r.sign() != Sign::NoSign {
            (q - 1, r + &a1)
        } else {
            (q, r)
        };
        let q1 = ptr_from_number(a, &q)?;
        let r1 = ptr_from_number(a, &r)?;
        let r: T::Ptr = a.new_pair(q1, r1)?;
        Ok(Reduction(cost, r))
    }
}

pub fn op_gr<T: Allocator>(a: &mut T, input: T::Ptr, _max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, ">")?;
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    let v0 = int_atom(&a0, ">")?;
    let v1 = int_atom(&a1, ">")?;
    let cost = GR_BASE_COST + (v0.len() + v1.len()) as Cost / GR_COST_PER_LIMB_DIVIDER;
    Ok(Reduction(
        cost,
        if number_from_u8(v0) > number_from_u8(v1) {
            a.one()
        } else {
            a.null()
        },
    ))
}

pub fn op_gr_bytes<T: Allocator>(a: &mut T, input: T::Ptr, _max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, ">s")?;
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    let v0 = atom(&a0, ">s")?;
    let v1 = atom(&a1, ">s")?;
    let cost = CMP_BASE_COST + (v0.len() + v1.len()) as Cost / CMP_COST_PER_LIMB_DIVIDER;
    Ok(Reduction(cost, if v0 > v1 { a.one() } else { a.null() }))
}

pub fn op_strlen<T: Allocator>(a: &mut T, input: T::Ptr, _max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "strlen")?;
    let a0 = args.first()?;
    let v0 = atom(&a0, "strlen")?;
    let size = v0.len();
    let size_num: Number = size.into();
    let size_node = ptr_from_number(a, &size_num)?;
    let cost = STRLEN_BASE_COST + size as Cost / STRLEN_COST_PER_BYTE_DIVIDER;
    Ok(Reduction(cost, size_node))
}

pub fn op_substr<T: Allocator>(a: &mut T, input: T::Ptr, _max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 3, "substr")?;
    let a0 = args.first()?;
    let s0 = atom(&a0, "substr")?;
    let rest = args.rest()?;
    let i1 = i32_atom(&rest.first()?, "substr")?;
    let rest = rest.rest()?;
    let i2 = i32_atom(&rest.first()?, "substr")?;
    let size = s0.len();
    if i2 < 0 || i1 < 0 || i2 as usize > size || i2 < i1 {
        args.err("invalid indices for substr")
    } else {
        let atom_node = a0.node;
        let r = a.new_substr(atom_node, i1 as u32, i2 as u32)?;
        let cost: Cost = 1;
        Ok(Reduction(cost, r))
    }
}

pub fn op_concat<T: Allocator>(a: &mut T, input: T::Ptr, max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    let mut cost = CONCAT_BASE_COST;
    let mut total_size: usize = 0;
    for arg in &args {
        cost += CONCAT_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        let blob = atom(&arg, "concat")?;
        total_size += blob.len();
    }
    let mut v: Vec<u8> = Vec::with_capacity(total_size);

    for arg in args {
        let blob = arg.atom().unwrap();
        v.extend_from_slice(blob);
    }
    cost += (total_size as Cost) / CONCAT_COST_PER_BYTE_DIVIDER;
    let r: T::Ptr = a.new_atom(&v)?;

    Ok(Reduction(cost, r))
}

pub fn op_ash<T: Allocator>(a: &mut T, input: T::Ptr, _max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, "ash")?;
    let a0 = args.first()?;
    let b0 = int_atom(&a0, "ash")?;
    let i0 = number_from_u8(b0);
    let l0 = b0.len();
    let rest = args.rest()?;
    let a1 = i32_atom(&rest.first()?, "ash")?;
    if a1.abs() > 65535 {
        return args.rest()?.first()?.err("shift too large");
    }

    let v: Number = if a1 > 0 { i0 << a1 } else { i0 >> -a1 };
    let l1 = limbs_for_int(&v);
    let r = ptr_from_number(a, &v)?;
    let cost = SHIFT_BASE_COST + ((l0 + l1) as Cost) / SHIFT_COST_PER_BYTE_DIVIDER;
    Ok(Reduction(cost, r))
}

pub fn op_lsh<T: Allocator>(a: &mut T, input: T::Ptr, _max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 2, "lsh")?;
    let a0 = args.first()?;
    let b0 = int_atom(&a0, "lsh")?;
    let i0 = BigUint::from_bytes_be(b0);
    let l0 = b0.len();
    let rest = args.rest()?;
    let a1 = i32_atom(&rest.first()?, "lsh")?;
    if a1.abs() > 65535 {
        return args.rest()?.first()?.err("shift too large");
    }

    let i0: Number = i0.into();

    let v: Number = if a1 > 0 { i0 << a1 } else { i0 >> -a1 };

    let l1 = limbs_for_int(&v);
    let r = ptr_from_number(a, &v)?;
    let cost = SHIFT_BASE_COST + ((l0 + l1) as Cost) / SHIFT_COST_PER_BYTE_DIVIDER;
    Ok(Reduction(cost, r))
}

fn binop_reduction<T: Allocator>(
    op_name: &str,
    a: &mut T,
    initial_value: Number,
    input: T::Ptr,
    max_cost: Cost,
    op_f: fn(&mut Number, &Number) -> (),
) -> Response<T::Ptr> {
    let mut total = initial_value;
    let mut arg_size: usize = 0;
    let mut cost = LOG_BASE_COST;
    for arg in Node::new(a, input) {
        let blob = int_atom(&arg, op_name)?;
        let n0 = number_from_u8(blob);
        op_f(&mut total, &n0);
        arg_size += blob.len();
        cost += LOG_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
    }
    cost += arg_size as Cost / LOG_COST_PER_LIMB_DIVIDER;
    let total = ptr_from_number(a, &total)?;
    Ok(Reduction(cost, total))
}

fn logand_op<T: Allocator>(a: &mut Number, b: &Number) {
    a.bitand_assign(b);
}

pub fn op_logand<T: Allocator>(a: &mut T, input: T::Ptr, max_cost: Cost) -> Response<T::Ptr> {
    let v: Number = (-1).into();
    binop_reduction("logand", a, v, input, max_cost, logand_op::<T>)
}

fn logior_op<T: Allocator>(a: &mut Number, b: &Number) {
    a.bitor_assign(b);
}

pub fn op_logior<T: Allocator>(a: &mut T, input: T::Ptr, max_cost: Cost) -> Response<T::Ptr> {
    let v: Number = (0).into();
    binop_reduction("logior", a, v, input, max_cost, logior_op::<T>)
}

fn logxor_op<T: Allocator>(a: &mut Number, b: &Number) {
    a.bitxor_assign(b);
}

pub fn op_logxor<T: Allocator>(a: &mut T, input: T::Ptr, max_cost: Cost) -> Response<T::Ptr> {
    let v: Number = (0).into();
    binop_reduction("logxor", a, v, input, max_cost, logxor_op::<T>)
}

pub fn op_lognot<T: Allocator>(a: &mut T, input: T::Ptr, _max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "lognot")?;
    let a0 = args.first()?;
    let v0 = int_atom(&a0, "lognot")?;
    let mut n: Number = number_from_u8(&v0);
    n = !n;
    let cost = LOGNOT_BASE_COST + (v0.len() as Cost) / LOGNOT_COST_PER_BYTE_DIVIDER;
    let r = ptr_from_number(a, &n)?;
    Ok(Reduction(cost, r))
}

pub fn op_not<T: Allocator>(a: &mut T, input: T::Ptr, _max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "not")?;
    let r: T::Ptr = args.from_bool(!args.first()?.as_bool()).node;
    let cost = BOOL_BASE_COST + BOOL_COST_PER_ARG;
    Ok(Reduction(cost, r))
}

pub fn op_any<T: Allocator>(a: &mut T, input: T::Ptr, max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    let mut cost = BOOL_BASE_COST;
    let mut is_any = false;
    for arg in &args {
        cost += BOOL_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        is_any = is_any || arg.as_bool();
    }
    let total: Node<T> = args.from_bool(is_any);
    Ok(Reduction(cost, total.node))
}

pub fn op_all<T: Allocator>(a: &mut T, input: T::Ptr, max_cost: Cost) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    let mut cost = BOOL_BASE_COST;
    let mut is_all = true;
    for arg in &args {
        cost += BOOL_COST_PER_ARG;
        check_cost(a, cost, max_cost)?;
        is_all = is_all && arg.as_bool();
    }
    let total: Node<T> = args.from_bool(is_all);
    Ok(Reduction(cost, total.node))
}

pub fn op_softfork<T: Allocator>(a: &mut T, input: T::Ptr, max_cost: Cost) -> Response<T::Ptr> {
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

pub fn op_pubkey_for_exp<T: Allocator>(
    a: &mut T,
    input: T::Ptr,
    _max_cost: Cost,
) -> Response<T::Ptr> {
    let args = Node::new(a, input);
    check_arg_count(&args, 1, "pubkey_for_exp")?;
    let a0 = args.first()?;

    let v0 = int_atom(&a0, "pubkey_for_exp")?;
    let exp: Number = mod_group_order(number_from_u8(&v0));
    let cost = PUBKEY_BASE_COST + (v0.len() as Cost) / PUBKEY_COST_PER_BYTE_DIVIDER;
    let exp: Scalar = number_to_scalar(exp);
    let point: G1Projective = G1Affine::generator() * exp;
    let point: G1Affine = point.into();

    Ok(Reduction(cost, a.new_atom(&point.to_compressed())?))
}

pub fn op_point_add<T: Allocator>(a: &mut T, input: T::Ptr, max_cost: Cost) -> Response<T::Ptr> {
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
    Ok(Reduction(cost, a.new_atom(&total.to_compressed())?))
}
