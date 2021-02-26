use crate::allocator::Allocator;
use crate::err_utils::err;
use crate::node::Node;
use crate::number::{number_from_signed_u8, Number};
use crate::reduction::EvalErr;

pub fn check_arg_count<T: Allocator>(
    args: &Node<T>,
    expected: i32,
    name: &str,
) -> Result<(), EvalErr<T::Ptr>> {
    let mut cnt = expected;
    // It would be nice to have a trait that wouldn't require us to copy every
    // node
    let mut ptr = args.clone();
    loop {
        match ptr.pair() {
            Some((_, next)) => {
                ptr = next.clone();
            }
            _ => {
                return if cnt == 0 {
                    Ok(())
                } else {
                    args.err(&format!(
                        "{} takes exactly {} argument{}",
                        name,
                        expected,
                        if expected == 1 { "" } else { "s" }
                    ))
                }
            }
        }
        cnt -= 1;
    }
}

pub fn int_atom<'a, T: Allocator>(
    args: &'a Node<T>,
    op_name: &str,
) -> Result<&'a [u8], EvalErr<T::Ptr>> {
    match args.atom() {
        Some(a) => Ok(a),
        _ => args.err(&format!("{} requires int args", op_name)),
    }
}

// rename to atom()
pub fn atom<'a, T: Allocator>(
    args: &'a Node<T>,
    op_name: &str,
) -> Result<&'a [u8], EvalErr<T::Ptr>> {
    match args.atom() {
        Some(a) => Ok(a),
        _ => args.err(&format!("{} on list", op_name)),
    }
}

pub fn two_ints<T: Allocator>(
    args: &Node<T>,
    op_name: &str,
) -> Result<(Number, usize, Number, usize), EvalErr<T::Ptr>> {
    check_arg_count(&args, 2, op_name)?;
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    let n0 = int_atom(&a0, op_name)?;
    let n1 = int_atom(&a1, op_name)?;
    Ok((
        number_from_signed_u8(n0),
        n0.len(),
        number_from_signed_u8(n1),
        n1.len(),
    ))
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
    match u32_from_u8_impl(buf, true) {
        Some(v) => Some(v as i32),
        None => None,
    }
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

pub fn i32_atom<A: Allocator>(args: &Node<A>, op_name: &str) -> Result<i32, EvalErr<A::Ptr>> {
    let buf = match args.atom() {
        Some(a) => a,
        _ => {
            return args.err(&format!("{} requires int32 args", op_name));
        }
    };
    match i32_from_u8(buf) {
        Some(v) => Ok(v),
        _ => args.err(&format!(
            "{} requires int32 args (with no leading zeros)",
            op_name
        )),
    }
}

impl<'a, A: Allocator> Node<'a, A> {
    pub fn first(&self) -> Result<Node<'a, A>, EvalErr<A::Ptr>> {
        match self.pair() {
            Some((p1, _)) => Ok(self.with_node(p1.node)),
            _ => self.err("first of non-cons"),
        }
    }

    pub fn rest(&self) -> Result<Node<'a, A>, EvalErr<A::Ptr>> {
        match self.pair() {
            Some((_, p2)) => Ok(self.with_node(p2.node)),
            _ => self.err("rest of non-cons"),
        }
    }

    pub fn err<T>(&self, msg: &str) -> Result<T, EvalErr<A::Ptr>> {
        err(self.node.clone(), msg)
    }
}
