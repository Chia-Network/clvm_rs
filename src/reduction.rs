use std::io;

use crate::allocator::NodePtr;
use crate::cost::Cost;

#[derive(Debug, Clone, PartialEq)]
pub struct EvalErr(pub NodePtr, pub String);

#[derive(Debug, PartialEq)]
pub struct Reduction(pub Cost, pub NodePtr);

pub type Response = Result<Reduction, EvalErr>;

impl From<EvalErr> for io::Error {
    fn from(v: EvalErr) -> Self {
        Self::new(io::ErrorKind::Other, v.1)
    }
}
