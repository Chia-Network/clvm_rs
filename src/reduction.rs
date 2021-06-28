use crate::allocator::NodePtr;
use crate::cost::Cost;

#[derive(Debug, Clone, PartialEq)]
pub struct EvalErr(pub NodePtr, pub String);

#[derive(Debug, PartialEq)]
pub struct Reduction(pub Cost, pub NodePtr);

pub type Response = Result<Reduction, EvalErr>;
