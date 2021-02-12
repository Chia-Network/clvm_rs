use crate::allocator::Allocator;
use crate::reduction::EvalErr;

pub fn err<T, P>(node: P, msg: &str) -> Result<T, EvalErr<P>> {
    Err(EvalErr(node, msg.into()))
}

pub fn u8_err<A: Allocator, T>(
    allocator: &A,
    node: &[u8],
    msg: &str,
) -> Result<T, EvalErr<A::Ptr>> {
    err(allocator.new_atom(node), msg)
}
