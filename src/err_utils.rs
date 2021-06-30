use crate::allocator::NodePtr;
use crate::reduction::EvalErr;

pub fn err<T>(node: NodePtr, msg: &str) -> Result<T, EvalErr> {
    Err(EvalErr(node, msg.into()))
}
