use crate::allocator::Allocator;
use crate::allocator::SExp::Atom;
use crate::cost::Cost;
use crate::err_utils::err;
use crate::node::Node;
use crate::number::Number;
use crate::reduction::EvalErr;
use crate::reduction::{Reduction, Response};
use bls12_381::Scalar;
use lazy_static::lazy_static;
use num_bigint::{BigUint, Sign};
use num_integer::Integer;

// We ascribe some additional cost per byte for operations that allocate new atoms
pub const MALLOC_COST_PER_BYTE: Cost = 10;

pub fn check_arg_count(args: &Node, expected: usize, name: &str) -> Result<(), EvalErr> {
    if arg_count(args, expected) != expected {
        err(
            args.node,
            &format!(
                "{name} takes exactly {expected} argument{}",
                if expected == 1 { "" } else { "s" }
            ),
        )
    } else {
        Ok(())
    }
}

pub fn arg_count(args: &Node, return_early_if_exceeds: usize) -> usize {
    let mut count = 0;
    // It would be nice to have a trait that wouldn't require us to copy every
    // node
    let mut ptr = args.clone();
    while let Some((_, next)) = ptr.pair() {
        ptr = next.clone();
        count += 1;
        if count > return_early_if_exceeds {
            break;
        };
    }
    count
}

#[test]
fn test_arg_count() {
    use crate::allocator::Allocator;

    let mut allocator = Allocator::new();
    let null = allocator.null();
    let ptr_0_args = null;
    let ptr_1_args = allocator.new_pair(null, ptr_0_args).unwrap();
    let ptr_2_args = allocator.new_pair(null, ptr_1_args).unwrap();
    let ptr_3_args = allocator.new_pair(null, ptr_2_args).unwrap();

    let count_0_args: Node = Node::new(&allocator, ptr_0_args);
    assert_eq!(arg_count(&count_0_args, 0), 0);
    assert_eq!(arg_count(&count_0_args, 1), 0);
    assert_eq!(arg_count(&count_0_args, 2), 0);

    let count_1_args: Node = Node::new(&allocator, ptr_1_args);
    assert_eq!(arg_count(&count_1_args, 0), 1);
    assert_eq!(arg_count(&count_1_args, 1), 1);
    assert_eq!(arg_count(&count_1_args, 2), 1);

    let count_2_args: Node = Node::new(&allocator, ptr_2_args);
    assert_eq!(arg_count(&count_2_args, 0), 1);
    assert_eq!(arg_count(&count_2_args, 1), 2);
    assert_eq!(arg_count(&count_2_args, 2), 2);
    assert_eq!(arg_count(&count_2_args, 3), 2);

    let count_3_args: Node = Node::new(&allocator, ptr_3_args);
    assert_eq!(arg_count(&count_3_args, 0), 1);
    assert_eq!(arg_count(&count_3_args, 1), 2);
    assert_eq!(arg_count(&count_3_args, 2), 3);
    assert_eq!(arg_count(&count_3_args, 3), 3);
    assert_eq!(arg_count(&count_3_args, 4), 3);
}

pub fn int_atom(args: Node, op_name: &str) -> Result<(Number, usize), EvalErr> {
    match args.sexp() {
        Atom() => Ok((
            args.allocator.number(args.node),
            args.allocator.atom_len(args.node),
        )),
        _ => err(args.node, &format!("{op_name} requires int args")),
    }
}

pub fn atom_len(args: Node, op_name: &str) -> Result<usize, EvalErr> {
    match args.sexp() {
        Atom() => Ok(args.allocator.atom_len(args.node)),
        _ => err(args.node, &format!("{op_name} requires an atom")),
    }
}

pub fn uint_atom<const SIZE: usize>(args: &Node, op_name: &str) -> Result<u64, EvalErr> {
    let bytes = match args.atom() {
        Some(a) => a,
        _ => {
            return err(args.node, &format!("{op_name} requires int arg"));
        }
    };

    if bytes.is_empty() {
        return Ok(0);
    }

    if (bytes[0] & 0x80) != 0 {
        return err(args.node, &format!("{op_name} requires positive int arg"));
    }

    // strip leading zeros
    let mut buf: &[u8] = bytes;
    while !buf.is_empty() && buf[0] == 0 {
        buf = &buf[1..];
    }

    if buf.len() > SIZE {
        return err(args.node, &format!("{op_name} requires u{} arg", SIZE * 8));
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
    assert!(uint_atom::<4>(&Node::new(&a, n), "test") == Ok(expected));
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
    assert!(uint_atom::<4>(&Node::new(&a, n), "test") == err(n, expected));
}

#[test]
fn test_uint_atom_4_pair() {
    use crate::allocator::Allocator;
    let mut a = Allocator::new();
    let n = a.new_atom(&[0, 0]).unwrap();
    let p = a.new_pair(n, n).unwrap();
    assert!(uint_atom::<4>(&Node::new(&a, p), "test") == err(p, "test requires int arg"));
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
    assert!(uint_atom::<8>(&Node::new(&a, n), "test") == Ok(expected));
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
    assert!(uint_atom::<8>(&Node::new(&a, n), "test") == err(n, expected));
}

#[test]
fn test_uint_atom_8_pair() {
    use crate::allocator::Allocator;
    let mut a = Allocator::new();
    let n = a.new_atom(&[0, 0]).unwrap();
    let p = a.new_pair(n, n).unwrap();
    assert!(uint_atom::<8>(&Node::new(&a, p), "test") == err(p, "test requires int arg"));
}

pub fn atom<'a>(args: Node<'a>, op_name: &str) -> Result<&'a [u8], EvalErr> {
    match args.atom() {
        Some(a) => Ok(a),
        _ => err(args.node, &format!("{op_name} on list")),
    }
}

pub fn two_ints(args: &Node, op_name: &str) -> Result<(Number, usize, Number, usize), EvalErr> {
    check_arg_count(args, 2, op_name)?;
    let (n0, n0_len) = int_atom(args.first()?, op_name)?;
    let (n1, n1_len) = int_atom(args.rest()?.first()?, op_name)?;
    Ok((n0, n0_len, n1, n1_len))
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

pub fn i32_atom(args: &Node, op_name: &str) -> Result<i32, EvalErr> {
    let buf = match args.atom() {
        Some(a) => a,
        _ => {
            return err(args.node, &format!("{op_name} requires int32 args"));
        }
    };
    match i32_from_u8(buf) {
        Some(v) => Ok(v),
        _ => err(
            args.node,
            &format!("{op_name} requires int32 args (with no leading zeros)"),
        ),
    }
}

impl<'a> Node<'a> {
    pub fn first(&self) -> Result<Node<'a>, EvalErr> {
        match self.pair() {
            Some((p1, _)) => Ok(self.with_node(p1.node)),
            _ => err(self.node, "first of non-cons"),
        }
    }

    pub fn rest(&self) -> Result<Node<'a>, EvalErr> {
        match self.pair() {
            Some((_, p2)) => Ok(self.with_node(p2.node)),
            _ => err(self.node, "rest of non-cons"),
        }
    }
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
