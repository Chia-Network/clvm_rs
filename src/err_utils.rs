use crate::allocator::Allocator;
use crate::reduction::EvalErr;

pub fn err<T, P>(node: P, msg: &str) -> Result<T, EvalErr<P>> {
    Err(EvalErr(node, msg.into()))
}

pub fn u8_err<A: Allocator, T>(
    allocator: &A,
    o: &A::AtomBuf,
    msg: &str,
) -> Result<T, EvalErr<A::Ptr>> {
    let op = allocator.buf(&o);
    let buf = op.to_vec();
    err(allocator.new_atom(&buf), msg)
}
