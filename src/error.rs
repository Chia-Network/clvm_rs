use crate::NodePtr;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, EvalErr>;

#[derive(Debug, Error, PartialEq)]
pub enum EvalErr {
    #[error("bad encoding")]
    SerializationError,

    #[error("Out of Memory")]
    OutOfMemory,

    #[error("path into atom")]
    PathIntoAtom,

    #[error("too many pairs")]
    TooManyPairs,

    #[error("Too Many Atoms")]
    TooManyAtoms,

    #[error("cost exceeded or below zero")]
    CostExceeded,

    #[error("unknown softfork extension")]
    UnknownSoftforkExtension,

    #[error("Softfork specified cost mismatch")]
    SoftforkCostMismatch,

    #[error("Internal Error: {1}")]
    InternalError(NodePtr, String),

    #[error("clvm raise")]
    Raise(NodePtr),

    #[error("Invalid Nil Terminator in operand list")]
    InvalidNilTerminator(NodePtr),

    #[error("Division by zero")]
    DivisionByZero(NodePtr),

    #[error("Value Stack Limit Reached")]
    ValueStackLimitReached(NodePtr),

    #[error("Environment Stack Limit Reached")]
    EnvironmentStackLimitReached(NodePtr),

    #[error("Shift too large")]
    ShiftTooLarge(NodePtr),

    #[error("Reserved operator")]
    Reserved(NodePtr),

    #[error("invalid operator")]
    Invalid(NodePtr),

    #[error("unimplemented operator")]
    Unimplemented(NodePtr),

    #[error("Operator Error: InvalidArg: {1}")]
    InvalidArg(NodePtr, String),

    #[error("Allocator Error: {0}")]
    Allocator(#[from] AllocatorErr),
}
impl From<std::io::Error> for EvalErr {
    fn from(_: std::io::Error) -> Self {
        EvalErr::SerializationError
    }
}

impl EvalErr {
    fn node(&self) -> Option<NodePtr> {
        match self {
            EvalErr::Raise(node) => Some(*node),
            EvalErr::InvalidNilTerminator(node) => Some(*node),
            EvalErr::DivisionByZero(node) => Some(*node),
            EvalErr::ShiftTooLarge(node) => Some(*node),
            EvalErr::ValueStackLimitReached(node) => Some(*node),
            EvalErr::EnvironmentStackLimitReached(node) => Some(*node),
            EvalErr::InternalError(node, _) => Some(*node),
            EvalErr::Reserved(node) => Some(*node),
            EvalErr::Invalid(node) => Some(*node),
            EvalErr::Unimplemented(node) => Some(*node),
            EvalErr::Allocator(alloc) => AllocatorErr::node(alloc),
            EvalErr::InvalidArg(node, _) => Some(*node),
            _ => None,
        }
    }

    pub fn node_ptr(&self) -> NodePtr {
        // This is a convenience function to get the node pointer
        self.node().unwrap_or_default()
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum AllocatorErr {
    #[error("InvalidArg: {1}")]
    InvalidArg(NodePtr, String),
}

impl AllocatorErr {
    pub fn node(&self) -> Option<NodePtr> {
        match self {
            AllocatorErr::InvalidArg(node, _) => Some(*node),
        }
    }
}
