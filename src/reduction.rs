use crate::cost::Cost;
use crate::allocator::NodePtr;

#[derive(Debug, Clone, PartialEq)]
pub struct EvalErr(pub NodePtr, pub String);

#[derive(Debug, PartialEq)]
pub struct Reduction<T>(pub Cost, pub T);

pub type Response<T> = Result<Reduction<T>, EvalErr>;
