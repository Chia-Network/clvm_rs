use crate::int_allocator::{IntAllocator, NodePtr, AtomBuf};
use crate::reduction::EvalErr;

pub fn err<T, P>(node: P, msg: &str) -> Result<T, EvalErr<P>> {
    Err(EvalErr(node, msg.into()))
}

// TODO: if we pass in NodePtr instead of AtomBuf, we don't have to allocate a
// new atom, and we could avoid the akward possibility of failing to allocate
// the error message
pub fn u8_err<T>(
    allocator: &mut IntAllocator,
    o: &AtomBuf,
    msg: &str,
) -> Result<T, EvalErr<NodePtr>> {
    let op = allocator.buf(o);
    let buf = op.to_vec();
    err(allocator.new_atom(&buf)?, msg)
}
