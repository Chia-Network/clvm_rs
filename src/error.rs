use crate::NodePtr;
use std::{fmt, io};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalErr(pub NodePtr, pub String);

impl fmt::Display for EvalErr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Error at {:?}: {}", self.0, self.1)
    }
}

impl std::error::Error for EvalErr {}

impl From<EvalErr> for io::Error {
    fn from(v: EvalErr) -> Self {
        Self::other(v.1)
    }
}
