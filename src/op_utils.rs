use num_bigint::BigUint;

use crate::allocator::Allocator;
use crate::err_utils::err;
use crate::node::Node;
use crate::number::{number_from_u8, Number};
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
    Ok((number_from_u8(n0), n0.len(), number_from_u8(n1), n1.len()))
}

pub fn uint_int<T: Allocator>(
    args: &Node<T>,
    op_name: &str,
) -> Result<(BigUint, usize, Number, usize), EvalErr<T::Ptr>> {
    check_arg_count(&args, 2, op_name)?;
    let a0 = args.first()?;
    let a1 = args.rest()?.first()?;
    let v0 = int_atom(&a0, op_name)?;
    let v1 = int_atom(&a1, op_name)?;
    Ok((
        BigUint::from_bytes_be(v0),
        v0.len(),
        number_from_u8(v1),
        v1.len(),
    ))
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
