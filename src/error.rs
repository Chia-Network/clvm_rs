use crate::NodePtr;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, EvalErr>;

#[derive(Debug, Error, PartialEq)]
pub enum EvalErr {
    #[error("bad encoding")]
    SerializationError,

    #[error("invalid backreference during deserialisation")]
    SerializationBackreferenceError,

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

    #[error("softfork specified cost mismatch")]
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

    #[error("InvalidOperatorArg: {1}")]
    InvalidOpArg(NodePtr, String),

    #[error("InvalidAllocatorArg: {1}")]
    InvalidAllocArg(NodePtr, String),

    #[error("bls_pairing_identity failed")]
    BLSPairingIdentityFailed(NodePtr),

    #[error("bls_verify failed")]
    BLSVerifyFailed(NodePtr),

    #[error("Secp256 Verify Error: failed")]
    Secp256Failed(NodePtr),
}
impl From<std::io::Error> for EvalErr {
    fn from(_: std::io::Error) -> Self {
        EvalErr::SerializationError
    }
}

impl EvalErr {
    fn node(&self) -> Option<NodePtr> {
        match self {
            EvalErr::InternalError(node, _) => Some(*node),
            EvalErr::Raise(node) => Some(*node),
            EvalErr::InvalidNilTerminator(node) => Some(*node),
            EvalErr::DivisionByZero(node) => Some(*node),
            EvalErr::ValueStackLimitReached(node) => Some(*node),
            EvalErr::EnvironmentStackLimitReached(node) => Some(*node),
            EvalErr::ShiftTooLarge(node) => Some(*node),
            EvalErr::Reserved(node) => Some(*node),
            EvalErr::Invalid(node) => Some(*node),
            EvalErr::Unimplemented(node) => Some(*node),
            EvalErr::InvalidOpArg(node, _) => Some(*node),
            EvalErr::InvalidAllocArg(node, _) => Some(*node),
            EvalErr::BLSPairingIdentityFailed(node) => Some(*node),
            EvalErr::BLSVerifyFailed(node) => Some(*node),
            EvalErr::Secp256Failed(node) => Some(*node),
            _ => None,
        }
    }

    pub fn node_ptr(&self) -> NodePtr {
        // This is a convenience function to get the node pointer
        self.node().unwrap_or_default()
    }
}
