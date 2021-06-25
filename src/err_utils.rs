use crate::allocator::{Allocator, AtomBuf, NodePtr};
use crate::reduction::EvalErr;

pub fn err<T>(node: NodePtr, msg: &str) -> Result<T, EvalErr> {
    Err(EvalErr(node, msg.into()))
}

// TODO: if we pass in NodePtr instead of AtomBuf, we don't have to allocate a
// new atom, and we could avoid the akward possibility of failing to allocate
// the error message
pub fn u8_err<T>(allocator: &mut Allocator, o: &AtomBuf, msg: &str) -> Result<T, EvalErr> {
    let op = allocator.buf(o);
    let buf = op.to_vec();
    err(allocator.new_atom(&buf)?, msg)
}
