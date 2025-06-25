use crate::allocator::NodePtr;
use crate::error::EvalErr;

pub fn err<T>(node: NodePtr, msg: &str) -> Result<T, EvalErr> {
    Err(EvalErr(node, msg.into()))
}
