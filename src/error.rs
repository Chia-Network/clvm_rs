use crate::NodePtr;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, EvalErr>;

#[derive(Debug, Error)]
pub enum EvalErr {
    #[error("bad decoding")]
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

    #[error("Unknown Softfork Extension")]
    UnknownSoftforkExtension,

    #[error("Softfork specified cost mismatch")]
    SoftforkSpecifiedCostMismatch,

    #[error("Internal Error: {1}")]
    InternalError(NodePtr, String),

    #[error("clvm raise")]
    Raise(NodePtr),

    #[error("Invalid Nil Terminator")]
    InvalidNilTerminator(NodePtr),

    #[error("Division by zero")]
    DivisionByZero(NodePtr),

    #[error("Divmod by zero")]
    DivmodByZero(NodePtr),

    #[error("Mod by zero")]
    ModByZero(NodePtr),

    #[error("ModPow with 0 Modulus")]
    ModPowZeroModulus(NodePtr),

    #[error("Value Stack Limit Reached")]
    ValueStackLimitReached(NodePtr),

    #[error("Environment Stack Limit Reached")]
    EnvironmentStackLimitReached(NodePtr),

    #[error("Shift too large")]
    ShiftTooLarge(NodePtr),

    #[error("Reserved operator")]
    Reserved(NodePtr),

    #[error("Invalid Operator")]
    Invalid(NodePtr),

    #[error("Unimplemented Operator")]
    Unimplemented(NodePtr),

    #[error("Operator Error: InvalidArg: {1}")]
    InvalidArg(NodePtr, String),

    #[error("CoinID Error: Invalid Parent Coin ID, not 32 bytes")]
    CoinIDParentCoinIdNot32Bytes(NodePtr),

    #[error("CoinID Error: Invalid Puzzle Hash, not 32 bytes")]
    CoinIDPuzzleHashNot32Bytes(NodePtr),

    #[error("CoinID Error: Invalid Amount: Amount is Negative")]
    CoinIDAmountNegative(NodePtr),

    #[error("CoinID Error: Invalid Amount: Amount has leading zeroes")]
    CoinIDAmountLeadingZeroes(NodePtr),

    #[error("CoinID Error: Invalid Amount: Amount exceeds max coin amount")]
    CoinIDAmountExceedsMaxCoinAmount(NodePtr),

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
            EvalErr::DivmodByZero(node) => Some(*node),
            EvalErr::ModByZero(node) => Some(*node),
            EvalErr::ModPowZeroModulus(node) => Some(*node),
            EvalErr::ShiftTooLarge(node) => Some(*node),
            EvalErr::ValueStackLimitReached(node) => Some(*node),
            EvalErr::EnvironmentStackLimitReached(node) => Some(*node),
            EvalErr::InternalError(node, _) => Some(*node),
            EvalErr::Reserved(node) => Some(*node),
            EvalErr::Invalid(node) => Some(*node),
            EvalErr::Unimplemented(node) => Some(*node),
            EvalErr::CoinIDPuzzleHashNot32Bytes(node) => Some(*node),
            EvalErr::CoinIDAmountNegative(node) => Some(*node),
            EvalErr::CoinIDAmountLeadingZeroes(node) => Some(*node),
            EvalErr::CoinIDAmountExceedsMaxCoinAmount(node) => Some(*node),
            EvalErr::CoinIDParentCoinIdNot32Bytes(node) => Some(*node),
            EvalErr::Allocator(alloc) => AllocatorErr::node(alloc),
            EvalErr::InvalidArg(node, _) => Some(*node),
            _ => None,
        }
    }
    pub fn combined_str(&self) -> String {
        // This is a convenience function to get the combined string representation of the error
        match self.node() {
            Some(node) => format!("{self}: {node:?}"),
            None => self.to_string(),
        }
    }
    pub fn node_ptr(&self) -> NodePtr {
        // This is a convenience function to get the node pointer
        self.node().unwrap_or_default()
    }
}

impl PartialEq<Self> for EvalErr {
    fn eq(&self, other: &Self) -> bool {
        self.combined_str() == other.combined_str()
    }
}
#[derive(Debug, Error)]
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
