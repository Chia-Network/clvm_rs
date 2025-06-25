use crate::allocator::NodePtr;
use crate::cost::Cost;
use crate::error::EvalErr;

#[derive(Debug, PartialEq, Eq)]
pub struct Reduction(pub Cost, pub NodePtr);

pub type Response = Result<Reduction, EvalErr>;
