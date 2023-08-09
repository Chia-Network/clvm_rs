use std::{fmt, io};

use crate::allocator::NodePtr;
use crate::cost::Cost;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalErr(pub NodePtr, pub String);

#[derive(Debug, PartialEq, Eq)]
pub struct Reduction(pub Cost, pub NodePtr);

pub type Response = Result<Reduction, EvalErr>;

impl fmt::Display for EvalErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error at {:?}: {}", self.0, self.1)
    }
}

impl std::error::Error for EvalErr {}

impl From<EvalErr> for io::Error {
    fn from(v: EvalErr) -> Self {
        Self::new(io::ErrorKind::Other, v.1)
    }
}
