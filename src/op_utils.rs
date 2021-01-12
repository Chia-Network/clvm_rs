use num_bigint::BigUint;

use crate::node::Node;
use crate::number::{number_from_u8, Number};
use crate::reduction::EvalErr;

pub fn check_arg_count<T>(args: &Node<T>, expected: i32, name: &str) -> Result<(), EvalErr<T>> {
    let mut cnt = expected;
    // It would be nice to have a trait that wouldn't require us to copy every
    // node
    let mut ptr = args.make_clone();
    loop {
        match ptr.pair() {
            Some((_, next)) => {
                ptr = next;
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

pub fn two_ints<T>(args: &Node<T>, op_name: &str) -> Result<(Number, Number), EvalErr<T>> {
    check_arg_count(&args, 2, op_name)?;
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
}

pub fn uint_int<T>(args: &Node<T>, op_name: &str) -> Result<(BigUint, Number), EvalErr<T>> {
    check_arg_count(&args, 2, op_name)?;
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
}
