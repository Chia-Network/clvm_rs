use std::error::Error;
use std::io;

use crate::allocator::NodePtr;
use crate::cost::Cost;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalErr(pub NodePtr, pub String);

#[derive(Debug, PartialEq, Eq)]
pub struct Reduction(pub Cost, pub NodePtr);

pub type Response = Result<Reduction, EvalErr>;

impl From<EvalErr> for io::Error {
    fn from(v: EvalErr) -> Self {
        Self::new(io::ErrorKind::Other, v.1)
    }
}

impl std::fmt::Display for EvalErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self.1)
    }
}

impl Error for EvalErr {}

impl From<Box<dyn Error>> for EvalErr {
    fn from(err: Box<dyn Error>) -> Self {
        EvalErr(0, err.to_string())
    }
}
