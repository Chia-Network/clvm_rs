use crate::allocator::{Allocator, NodePtr, SExp};
use crate::cost::Cost;
use crate::err_utils::err;
use crate::number::Number;
use crate::reduction::EvalErr;
use crate::reduction::{Reduction, Response};
use bls12_381::Scalar;
use lazy_static::lazy_static;
use num_bigint::{BigUint, Sign};
use num_integer::Integer;

// We ascribe some additional cost per byte for operations that allocate new atoms
pub const MALLOC_COST_PER_BYTE: Cost = 10;

pub fn get_args<const N: usize>(
    a: &Allocator,
    args: NodePtr,
    name: &str,
) -> Result<[NodePtr; N], EvalErr> {
    let mut next = args;
    let mut counter = 0;
    let mut ret: [NodePtr; N] = [0; N];

    while let Some((first, rest)) = a.next(next) {
        next = rest;
        if counter == N {
            return err(
                args,
                &format!(
                    "{name} takes exactly {N} argument{}",
                    if N == 1 { "" } else { "s" }
                ),
            );
        }
        ret[counter] = first;
        counter += 1;
    }

    if counter != N {
        err(
            args,
            &format!(
                "{name} takes exactly {N} argument{}",
                if N == 1 { "" } else { "s" }
            ),
        )
    } else {
        Ok(ret)
    }
}

#[test]
fn test_get_args() {
    let mut a = Allocator::new();
    let a0 = a.new_number(42.into()).unwrap();
    let a1 = a.new_number(1337.into()).unwrap();
    let a2 = a.new_number(0.into()).unwrap();
    let a3 = a.new_atom(&[]).unwrap();
    let args0 = a.null();
    let args1 = a.new_pair(a3, args0).unwrap();
    let args2 = a.new_pair(a2, args1).unwrap();
    let args3 = a.new_pair(a1, args2).unwrap();
    let args4 = a.new_pair(a0, args3).unwrap();

    assert_eq!(get_args::<4>(&a, args4, "test").unwrap(), [a0, a1, a2, a3]);

    let r = get_args::<3>(&a, args4, "test").unwrap_err();
    assert_eq!(r.0, args4);
    assert_eq!(r.1, "test takes exactly 3 arguments");

    let r = get_args::<5>(&a, args4, "test").unwrap_err();
    assert_eq!(r.0, args4);
    assert_eq!(r.1, "test takes exactly 5 arguments");

    let r = get_args::<4>(&a, args3, "test").unwrap_err();
    assert_eq!(r.0, args3);
    assert_eq!(r.1, "test takes exactly 4 arguments");

    let r = get_args::<4>(&a, args2, "test").unwrap_err();
    assert_eq!(r.0, args2);
    assert_eq!(r.1, "test takes exactly 4 arguments");

    let r = get_args::<1>(&a, args2, "test").unwrap_err();
    assert_eq!(r.0, args2);
    assert_eq!(r.1, "test takes exactly 1 argument");
}

pub fn get_varargs<const N: usize>(
    a: &Allocator,
    args: NodePtr,
    name: &str,
) -> Result<([NodePtr; N], usize), EvalErr> {
    let mut next = args;
    let mut counter = 0;
    let mut ret: [NodePtr; N] = [0; N];

    while let Some((first, rest)) = a.next(next) {
        next = rest;
        if counter == N {
            return err(
                args,
                &format!(
                    "{name} takes no more than {N} argument{}",
                    if N == 1 { "" } else { "s" }
                ),
            );
        }
        ret[counter] = first;
        counter += 1;
    }

    Ok((ret, counter))
}

#[test]
fn test_get_varargs() {
    let mut a = Allocator::new();
    let a0 = a.new_number(42.into()).unwrap();
    let a1 = a.new_number(1337.into()).unwrap();
    let a2 = a.new_number(0.into()).unwrap();
    let a3 = a.new_atom(&[]).unwrap();
    let args0 = a.null();
    let args1 = a.new_pair(a3, args0).unwrap();
    let args2 = a.new_pair(a2, args1).unwrap();
    let args3 = a.new_pair(a1, args2).unwrap();
    let args4 = a.new_pair(a0, args3).unwrap();

    // happy path
    assert_eq!(
        get_varargs::<4>(&a, args4, "test").unwrap(),
        ([a0, a1, a2, a3], 4)
    );
    assert_eq!(
        get_varargs::<4>(&a, args3, "test").unwrap(),
        ([a1, a2, a3, 0], 3)
    );
    assert_eq!(
        get_varargs::<4>(&a, args2, "test").unwrap(),
        ([a2, a3, 0, 0], 2)
    );
    assert_eq!(
        get_varargs::<4>(&a, args1, "test").unwrap(),
        ([a3, 0, 0, 0], 1)
    );
    assert_eq!(
        get_varargs::<4>(&a, args0, "test").unwrap(),
        ([0, 0, 0, 0], 0)
    );

    let r = get_varargs::<3>(&a, args4, "test").unwrap_err();
    assert_eq!(r.0, args4);
    assert_eq!(r.1, "test takes no more than 3 arguments");

    let r = get_varargs::<1>(&a, args4, "test").unwrap_err();
    assert_eq!(r.0, args4);
    assert_eq!(r.1, "test takes no more than 1 argument");
}

pub fn nullp(a: &Allocator, n: NodePtr) -> bool {
    match a.sexp(n) {
        SExp::Atom() => a.atom_len(n) == 0,
        _ => false,
    }
}

#[test]
fn test_nullp() {
    let mut a = Allocator::new();
    let a0 = a.new_number(42.into()).unwrap();
    let a1 = a.new_number(1337.into()).unwrap();
    let a3 = a.new_number(0.into()).unwrap();
    let a4 = a.new_atom(&[]).unwrap();
    let a5 = a.null();
    let pair = a.new_pair(a0, a1).unwrap();
    assert!(!nullp(&a, pair));
    assert!(!nullp(&a, a0));
    assert!(!nullp(&a, a1));
    assert!(nullp(&a, a3));
    assert!(nullp(&a, a4));
    assert!(nullp(&a, a5));
}

pub fn first(a: &Allocator, n: NodePtr) -> Result<NodePtr, EvalErr> {
    match a.sexp(n) {
        SExp::Pair(first, _) => Ok(first),
        _ => err(n, "first of non-cons"),
    }
}

#[test]
fn test_first() {
    let mut a = Allocator::new();
    let a0 = a.new_number(42.into()).unwrap();
    let a1 = a.new_number(1337.into()).unwrap();
    let pair = a.new_pair(a0, a1).unwrap();
    assert_eq!(first(&a, pair).unwrap(), a0);

    let r = first(&a, a0).unwrap_err();
    assert_eq!(r.0, a0);
    assert_eq!(r.1, "first of non-cons");
}

pub fn rest(a: &Allocator, n: NodePtr) -> Result<NodePtr, EvalErr> {
    match a.sexp(n) {
        SExp::Pair(_, rest) => Ok(rest),
        _ => err(n, "rest of non-cons"),
    }
}

#[test]
fn test_rest() {
    let mut a = Allocator::new();
    let a0 = a.new_number(42.into()).unwrap();
    let a1 = a.new_number(1337.into()).unwrap();
    let pair = a.new_pair(a0, a1).unwrap();
    assert_eq!(rest(&a, pair).unwrap(), a1);

    let r = rest(&a, a0).unwrap_err();
    assert_eq!(r.0, a0);
    assert_eq!(r.1, "rest of non-cons");
}

pub fn int_atom(a: &Allocator, args: NodePtr, op_name: &str) -> Result<(Number, usize), EvalErr> {
    match a.sexp(args) {
        SExp::Atom() => Ok((a.number(args), a.atom_len(args))),
        _ => err(args, &format!("{op_name} requires int args")),
    }
}

#[cfg(test)]
#[rstest]
#[case(0.into(), (0.into(), 0))]
#[case(1.into(), (1.into(), 1))]
#[case(42.into(), (42.into(), 1))]
#[case(1337.into(), (1337.into(), 2))]
#[case(0x5fffff.into(), (0x5fffff.into(), 3))]
#[case(0xffffff.into(), (0xffffff.into(), 4))]
fn test_int_atom(#[case] value: Number, #[case] expected: (Number, usize)) {
    let mut a = Allocator::new();
    let a0 = a.new_number(value).unwrap();
    assert_eq!(int_atom(&a, a0, "test").unwrap(), expected);
}

#[test]
fn test_int_atom_failure() {
    let mut a = Allocator::new();
    let a0 = a.new_number(42.into()).unwrap();
    let a1 = a.new_number(1337.into()).unwrap();
    let pair = a.new_pair(a0, a1).unwrap();
    let r = int_atom(&a, pair, "test").unwrap_err();
    assert_eq!(r.0, pair);
    assert_eq!(r.1, "test requires int args");
}

pub fn atom_len(a: &Allocator, args: NodePtr, op_name: &str) -> Result<usize, EvalErr> {
    match a.sexp(args) {
        SExp::Atom() => Ok(a.atom_len(args)),
        _ => err(args, &format!("{op_name} requires an atom")),
    }
}

#[test]
fn test_atom_len() {
    let mut a = Allocator::new();

    let a0 = a.new_number(42.into()).unwrap();
    let a1 = a.new_number(1337.into()).unwrap();
    let pair = a.new_pair(a0, a1).unwrap();

    let r = atom_len(&a, pair, "test").unwrap_err();
    assert_eq!(r.0, pair);
    assert_eq!(r.1, "test requires an atom");

    assert_eq!(atom_len(&a, a0, "test").unwrap(), 1);
    assert_eq!(atom_len(&a, a1, "test").unwrap(), 2);
}

pub fn uint_atom<const SIZE: usize>(
    a: &Allocator,
    args: NodePtr,
    op_name: &str,
) -> Result<u64, EvalErr> {
    let bytes = match a.sexp(args) {
        SExp::Atom() => a.atom(args),
        _ => {
            return err(args, &format!("{op_name} requires int arg"));
        }
    };

    if bytes.is_empty() {
        return Ok(0);
    }

    if (bytes[0] & 0x80) != 0 {
        return err(args, &format!("{op_name} requires positive int arg"));
    }

    // strip leading zeros
    let mut buf: &[u8] = bytes;
    while !buf.is_empty() && buf[0] == 0 {
        buf = &buf[1..];
    }

    if buf.len() > SIZE {
        return err(args, &format!("{op_name} requires u{} arg", SIZE * 8));
    }

    let mut ret = 0;
    for b in buf {
        ret <<= 8;
        ret |= *b as u64;
    }
    Ok(ret)
}

#[cfg(test)]
use rstest::rstest;

// u32, 4 bytes
#[cfg(test)]
#[rstest]
#[case(&[0], 0)]
#[case(&[0,0,0,1], 1)]
#[case(&[0,0xff,0xff,0xff,0xff], 0xffffffff)]
#[case(&[0,0,0,0,0,0xff,0xff,0xff,0xff], 0xffffffff)]
#[case(&[0x7f,0xff], 0x7fff)]
#[case(&[0x7f,0xff, 0xff], 0x7fffff)]
#[case(&[0x7f,0xff,0xff, 0xff], 0x7fffffff)]
#[case(&[0x01,0x02,0x03, 0x04], 0x1020304)]
#[case(&[] as &[u8], 0)]
fn test_uint_atom_4_success(#[case] buf: &[u8], #[case] expected: u64) {
    use crate::allocator::Allocator;
    let mut a = Allocator::new();
    let n = a.new_atom(buf).unwrap();
    assert!(uint_atom::<4>(&a, n, "test") == Ok(expected));
}

// u32, 4 bytes
#[cfg(test)]
#[rstest]
#[case(&[0xff,0xff,0xff,0xff], "test requires positive int arg")]
#[case(&[0xff], "test requires positive int arg")]
#[case(&[0x80], "test requires positive int arg")]
#[case(&[0x80,0,0,0], "test requires positive int arg")]
#[case(&[1, 0xff,0xff,0xff,0xff], "test requires u32 arg")]
fn test_uint_atom_4_failure(#[case] buf: &[u8], #[case] expected: &str) {
    use crate::allocator::Allocator;
    let mut a = Allocator::new();
    let n = a.new_atom(buf).unwrap();
    assert!(uint_atom::<4>(&a, n, "test") == err(n, expected));
}

#[test]
fn test_uint_atom_4_pair() {
    use crate::allocator::Allocator;
    let mut a = Allocator::new();
    let n = a.new_atom(&[0, 0]).unwrap();
    let p = a.new_pair(n, n).unwrap();
    assert!(uint_atom::<4>(&a, p, "test") == err(p, "test requires int arg"));
}

// u64, 8 bytes
#[cfg(test)]
#[rstest]
#[case(&[0], 0)]
#[case(&[0,0,0,1], 1)]
#[case(&[0,0xff,0xff,0xff,0xff], 0xffffffff)]
#[case(&[0,0,0,0,0xff,0xff,0xff,0xff], 0xffffffff)]
#[case(&[0x7f, 0xff], 0x7fff)]
#[case(&[0x7f, 0xff, 0xff], 0x7fffff)]
#[case(&[0x7f, 0xff,0xff, 0xff], 0x7fffffff)]
#[case(&[0x7f, 0xff,0xff, 0xff, 0xff], 0x7fffffffff)]
#[case(&[0x7f, 0xff,0xff, 0xff, 0xff, 0xff], 0x7fffffffffff)]
#[case(&[0x7f, 0xff,0xff, 0xff, 0xff, 0xff, 0xff], 0x7fffffffffffff)]
#[case(&[0x7f, 0xff,0xff, 0xff, 0xff, 0xff, 0xff, 0xff], 0x7fffffffffffffff)]
#[case(&[0x01, 0x02,0x03, 0x04, 0x05, 0x06, 0x07, 0x08 ], 0x102030405060708)]
#[case(&[] as &[u8], 0)]
fn test_uint_atom_8_success(#[case] buf: &[u8], #[case] expected: u64) {
    use crate::allocator::Allocator;
    let mut a = Allocator::new();
    let n = a.new_atom(buf).unwrap();
    assert!(uint_atom::<8>(&a, n, "test") == Ok(expected));
}

// u64, 8 bytes
#[cfg(test)]
#[rstest]
#[case(&[0xff,0xff,0xff,0xff], "test requires positive int arg")]
#[case(&[0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff], "test requires positive int arg")]
#[case(&[0xff], "test requires positive int arg")]
#[case(&[0x80], "test requires positive int arg")]
#[case(&[0x80,0,0,0], "test requires positive int arg")]
#[case(&[1,0xff,0xff,0xff,0xff,0xff,0xff,0xff,0xff], "test requires u64 arg")]
fn test_uint_atom_8_failure(#[case] buf: &[u8], #[case] expected: &str) {
    use crate::allocator::Allocator;
    let mut a = Allocator::new();
    let n = a.new_atom(buf).unwrap();
    assert!(uint_atom::<8>(&a, n, "test") == err(n, expected));
}

#[test]
fn test_uint_atom_8_pair() {
    use crate::allocator::Allocator;
    let mut a = Allocator::new();
    let n = a.new_atom(&[0, 0]).unwrap();
    let p = a.new_pair(n, n).unwrap();
    assert!(uint_atom::<8>(&a, p, "test") == err(p, "test requires int arg"));
}

pub fn atom<'a>(a: &'a Allocator, n: NodePtr, op_name: &str) -> Result<&'a [u8], EvalErr> {
    match a.sexp(n) {
        SExp::Atom() => Ok(a.atom(n)),
        _ => err(n, &format!("{op_name} on list")),
    }
}

fn u32_from_u8_impl(buf: &[u8], signed: bool) -> Option<u32> {
    if buf.is_empty() {
        return Some(0);
    }

    // too many bytes for u32
    if buf.len() > 4 {
        return None;
    }

    let sign_extend = (buf[0] & 0x80) != 0;
    let mut ret: u32 = if signed && sign_extend { 0xffffffff } else { 0 };
    for b in buf {
        ret <<= 8;
        ret |= *b as u32;
    }
    Some(ret)
}

pub fn u32_from_u8(buf: &[u8]) -> Option<u32> {
    u32_from_u8_impl(buf, false)
}

#[test]
fn test_u32_from_u8() {
    assert_eq!(u32_from_u8(&[]), Some(0));
    assert_eq!(u32_from_u8(&[0xcc]), Some(0xcc));
    assert_eq!(u32_from_u8(&[0xcc, 0x55]), Some(0xcc55));
    assert_eq!(u32_from_u8(&[0xcc, 0x55, 0x88]), Some(0xcc5588));
    assert_eq!(u32_from_u8(&[0xcc, 0x55, 0x88, 0xf3]), Some(0xcc5588f3));

    assert_eq!(u32_from_u8(&[0xff]), Some(0xff));
    assert_eq!(u32_from_u8(&[0xff, 0xff]), Some(0xffff));
    assert_eq!(u32_from_u8(&[0xff, 0xff, 0xff]), Some(0xffffff));
    assert_eq!(u32_from_u8(&[0xff, 0xff, 0xff, 0xff]), Some(0xffffffff));

    // leading zeros are not stripped, and not allowed beyond 4 bytes
    assert_eq!(u32_from_u8(&[0x00]), Some(0));
    assert_eq!(u32_from_u8(&[0x00, 0x00]), Some(0));
    assert_eq!(u32_from_u8(&[0x00, 0xcc, 0x55, 0x88]), Some(0xcc5588));
    assert_eq!(u32_from_u8(&[0x00, 0x00, 0xcc, 0x55, 0x88]), None);
    assert_eq!(u32_from_u8(&[0x00, 0xcc, 0x55, 0x88, 0xf3]), None);

    // overflow, too many bytes
    assert_eq!(u32_from_u8(&[0x01, 0xcc, 0x55, 0x88, 0xf3]), None);
    assert_eq!(u32_from_u8(&[0x01, 0x00, 0x00, 0x00, 0x00]), None);
    assert_eq!(u32_from_u8(&[0x7d, 0xcc, 0x55, 0x88, 0xf3]), None);
}

pub fn i32_from_u8(buf: &[u8]) -> Option<i32> {
    u32_from_u8_impl(buf, true).map(|v| v as i32)
}

#[test]
fn test_i32_from_u8() {
    assert_eq!(i32_from_u8(&[]), Some(0));
    assert_eq!(i32_from_u8(&[0xcc]), Some(-52));
    assert_eq!(i32_from_u8(&[0xcc, 0x55]), Some(-13227));
    assert_eq!(i32_from_u8(&[0xcc, 0x55, 0x88]), Some(-3385976));
    assert_eq!(i32_from_u8(&[0xcc, 0x55, 0x88, 0xf3]), Some(-866809613));

    assert_eq!(i32_from_u8(&[0xff]), Some(-1));
    assert_eq!(i32_from_u8(&[0xff, 0xff]), Some(-1));
    assert_eq!(i32_from_u8(&[0xff, 0xff, 0xff]), Some(-1));
    assert_eq!(i32_from_u8(&[0xff, 0xff, 0xff, 0xff]), Some(-1));

    // leading zeros are not stripped, and not allowed beyond 4 bytes
    assert_eq!(i32_from_u8(&[0x00]), Some(0));
    assert_eq!(i32_from_u8(&[0x00, 0x00]), Some(0));
    assert_eq!(i32_from_u8(&[0x00, 0xcc, 0x55, 0x88]), Some(0xcc5588));
    assert_eq!(i32_from_u8(&[0x00, 0x00, 0xcc, 0x55, 0x88]), None);
    assert_eq!(i32_from_u8(&[0x00, 0xcc, 0x55, 0x88, 0xf3]), None);

    // overflow, it doesn't really matter whether the bytes are 0 or not, any
    // atom larger than 4 bytes is rejected
    assert_eq!(i32_from_u8(&[0x01, 0xcc, 0x55, 0x88, 0xf3]), None);
    assert_eq!(i32_from_u8(&[0x01, 0x00, 0x00, 0x00, 0x00]), None);
    assert_eq!(i32_from_u8(&[0x7d, 0xcc, 0x55, 0x88, 0xf3]), None);
}

pub fn u64_from_bytes(buf: &[u8]) -> u64 {
    if buf.is_empty() {
        return 0;
    }

    let mut ret: u64 = 0;
    for b in buf {
        ret <<= 8;
        ret |= *b as u64;
    }
    ret
}

#[test]
fn test_u64_from_bytes() {
    assert_eq!(u64_from_bytes(&[]), 0);
    assert_eq!(u64_from_bytes(&[0xcc]), 0xcc);
    assert_eq!(u64_from_bytes(&[0xcc, 0x55]), 0xcc55);
    assert_eq!(u64_from_bytes(&[0xcc, 0x55, 0x88]), 0xcc5588);
    assert_eq!(u64_from_bytes(&[0xcc, 0x55, 0x88, 0xf3]), 0xcc5588f3);

    assert_eq!(u64_from_bytes(&[0xff]), 0xff);
    assert_eq!(u64_from_bytes(&[0xff, 0xff]), 0xffff);
    assert_eq!(u64_from_bytes(&[0xff, 0xff, 0xff]), 0xffffff);
    assert_eq!(u64_from_bytes(&[0xff, 0xff, 0xff, 0xff]), 0xffffffff);

    assert_eq!(u64_from_bytes(&[0x00]), 0);
    assert_eq!(u64_from_bytes(&[0x00, 0x00]), 0);
    assert_eq!(u64_from_bytes(&[0x00, 0xcc, 0x55, 0x88]), 0xcc5588);
    assert_eq!(u64_from_bytes(&[0x00, 0x00, 0xcc, 0x55, 0x88]), 0xcc5588);
    assert_eq!(u64_from_bytes(&[0x00, 0xcc, 0x55, 0x88, 0xf3]), 0xcc5588f3);

    assert_eq!(
        u64_from_bytes(&[0xcc, 0x55, 0x88, 0xf3, 0xcc, 0x55, 0x88, 0xf3]),
        0xcc5588f3cc5588f3
    );
}

pub fn i32_atom(a: &Allocator, args: NodePtr, op_name: &str) -> Result<i32, EvalErr> {
    let buf = match a.sexp(args) {
        SExp::Atom() => a.atom(args),
        _ => {
            return err(args, &format!("{op_name} requires int32 args"));
        }
    };
    match i32_from_u8(buf) {
        Some(v) => Ok(v),
        _ => err(
            args,
            &format!("{op_name} requires int32 args (with no leading zeros)"),
        ),
    }
}

#[test]
fn test_i32_atom() {
    let mut a = Allocator::new();

    let a0 = a.new_number(42.into()).unwrap();
    let a1 = a.new_number(1337.into()).unwrap();

    let pair = a.new_pair(a0, a1).unwrap();

    let r = i32_atom(&a, pair, "test").unwrap_err();
    assert_eq!(r.0, pair);
    assert_eq!(r.1, "test requires int32 args");

    assert_eq!(i32_atom(&a, a0, "test").unwrap(), 42);
    assert_eq!(i32_atom(&a, a1, "test").unwrap(), 1337);

    let a2 = a.new_number(0x100000000_i64.into()).unwrap();
    let r = i32_atom(&a, a2, "test").unwrap_err();
    assert_eq!(r.0, a2);
    assert_eq!(r.1, "test requires int32 args (with no leading zeros)");

    let a3 = a.new_number((-0xffffffff_i64).into()).unwrap();
    let r = i32_atom(&a, a3, "test").unwrap_err();
    assert_eq!(r.0, a3);
    assert_eq!(r.1, "test requires int32 args (with no leading zeros)");
}

pub fn number_to_scalar(n: Number) -> Scalar {
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

pub fn new_atom_and_cost(a: &mut Allocator, cost: Cost, buf: &[u8]) -> Response {
    let c = buf.len() as Cost * MALLOC_COST_PER_BYTE;
    Ok(Reduction(cost + c, a.new_atom(buf)?))
}

pub fn mod_group_order(n: Number) -> Number {
    let order = GROUP_ORDER.clone();
    let mut remainder = n.mod_floor(&order);
    if remainder.sign() == Sign::Minus {
        remainder += order;
    }
    remainder
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
