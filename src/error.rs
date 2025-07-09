use crate::NodePtr;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, EvalErr>;

#[derive(Debug, Error)]
pub enum EvalErr {
    #[error("Internal Error: {1}")]
    InternalError(NodePtr, String),

    #[error("bad decoding")]
    SerializationError,

    #[error("Out of Memory")]
    OutOfMemory,

    #[error("cost exceeded")]
    CostExceeded,

    #[error("Cost must be greater than zero.")]
    CostBelowZero,

    #[error("too many pairs")]
    TooManyPairs,

    #[error("Too Many Atoms")]
    TooManyAtoms,

    #[error("path into atom")]
    PathIntoAtom,

    #[error("Unknown Softfork Extension")]
    UnknownSoftforkExtension,

    #[error("Softfork specified cost mismatch")]
    SoftforkSpecifiedCostMismatch,

    #[error("clvm raise")]
    Raise(NodePtr),

    #[error("in ((X)...) syntax X must be lone atom")]
    InPairMustBeLoneAtom(NodePtr),

    #[error("Invalid Nil Terminator")]
    InvalidNilTerminator(NodePtr),

    #[error("First of non-cons")]
    FirstOfNonCons(NodePtr),

    #[error("Rest of non-cons")]
    RestOfNonCons(NodePtr),

    #[error("Division by zero")]
    DivisionByZero(NodePtr),

    #[error("Divmod by zero")]
    DivmodByZero(NodePtr),

    #[error("Mod by zero")]
    ModByZero(NodePtr),

    #[error("ModPow with Negative Exponent")]
    ModPowNegativeExponent(NodePtr),

    #[error("ModPow with 0 Modulus")]
    ModPowZeroModulus(NodePtr),

    #[error("Shift too large")]
    ShiftTooLarge(NodePtr),

    #[error("Value Stack Limit Reached")]
    ValueStackLimitReached(NodePtr),

    #[error("Environment Stack Limit Reached")]
    EnvironmentStackLimitReached(NodePtr),

    // Grouped errors
    #[error("Operator Error: {0}")]
    Operator(#[from] OperatorError),

    #[error("Allocator Error: {0}")]
    Allocator(#[from] AllocatorError),
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
            EvalErr::InPairMustBeLoneAtom(node) => Some(*node),
            EvalErr::InvalidNilTerminator(node) => Some(*node),
            EvalErr::FirstOfNonCons(node) => Some(*node),
            EvalErr::RestOfNonCons(node) => Some(*node),
            EvalErr::DivisionByZero(node) => Some(*node),
            EvalErr::DivmodByZero(node) => Some(*node),
            EvalErr::ModByZero(node) => Some(*node),
            EvalErr::ModPowNegativeExponent(node) => Some(*node),
            EvalErr::ModPowZeroModulus(node) => Some(*node),
            EvalErr::ShiftTooLarge(node) => Some(*node),
            EvalErr::ValueStackLimitReached(node) => Some(*node),
            EvalErr::EnvironmentStackLimitReached(node) => Some(*node),
            EvalErr::InternalError(node, _) => Some(*node),
            EvalErr::Operator(op) => OperatorError::node(op),
            EvalErr::Allocator(alloc) => AllocatorError::node(alloc),
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

// Operator Errors
#[derive(Debug, Error)]
pub enum OperatorError {
    #[error("Reserved operator")]
    Reserved(NodePtr),

    #[error("Invalid Operator")]
    Invalid(NodePtr),

    #[error("Unimplemented Operator")]
    Unimplemented(NodePtr),

    #[error("Requires Int Argument: {1}")]
    RequiresIntArgument(NodePtr, String),

    #[error("{1} Requires Positive Int Argument")]
    RequiresPositiveIntArgument(NodePtr, String),

    #[error("{1} Requires Int32 args (with no leading zeros)")]
    RequiresInt32Args(NodePtr, String),

    #[error("{1} Requires Int64 args (with no leading zeros)")]
    RequiresInt64Args(NodePtr, String),

    #[error("{1} Requires {2} arguments")]
    RequiresArgs(NodePtr, String, u32),

    #[error("{1} takes no more then {2} arguments")]
    TakesNoMoreThanArgs(NodePtr, String, u32),

    #[error("{1} requires an atom")]
    RequiresAtom(NodePtr, String),

    #[error("{1} used on list")]
    UsedOnList(NodePtr, String),

    #[error("{1} takes exactly {2} argument(s)")]
    TakesExactlyArgs(NodePtr, String, u32),

    #[error("Expected Atom, got Pair")]
    ExpectedAtomGotPair(NodePtr),

    #[error("Substring takes exactly 2 or 3 arguments, got {1}")]
    InvalidArgs2or3(NodePtr, u32),

    #[error("Invalid Indices for Substring")]
    InvalidIndices(NodePtr),

    #[error("concat on list")]
    ConcatOnList(NodePtr),

    #[error("atom is not a valid G1 point")]
    NotValidG1Point(NodePtr),

    #[error("G1_map takes exactly 1 or 2 arguments, got {1}")]
    G1MapInvalidArgs(NodePtr, u32),

    #[error("atom is not a valid G2 point")]
    NotValidG2Point(NodePtr),

    #[error("G2_map takes exactly 1 or 2 arguments, got {1}")]
    G2MapInvalidArgs(NodePtr, u32),

    #[error("atom is not G2 size (96 bytes)")]
    NotG2Size(NodePtr),

    #[error("bls_pairing_identity failed")]
    BLSPairingIdentityFailed(NodePtr),

    #[error("bls_verify failed")]
    BLSVerifyFailed(NodePtr),

    #[error("CoinID Error")]
    CoinID(#[from] CoinIDError),

    #[error("Secp256k1 Verify Error: {0}")]
    Secp256k1Verify(#[from] Secp256k1verifyError),

    #[error("Secp256r1 Verify Error: {0}")]
    Secp256r1Verify(#[from] Secp256r1verifyError),
}

impl OperatorError {
    pub fn node(&self) -> Option<NodePtr> {
        // All OperatorErrors have a NodePtr as a first argument
        match self {
            OperatorError::Reserved(node) => Some(*node),
            OperatorError::Invalid(node) => Some(*node),
            OperatorError::Unimplemented(node) => Some(*node),
            OperatorError::RequiresIntArgument(node, _) => Some(*node),
            OperatorError::RequiresPositiveIntArgument(node, _) => Some(*node),
            OperatorError::RequiresInt32Args(node, _) => Some(*node),
            OperatorError::RequiresInt64Args(node, _) => Some(*node),
            OperatorError::RequiresArgs(node, _, _) => Some(*node),
            OperatorError::TakesNoMoreThanArgs(node, _, _) => Some(*node),
            OperatorError::RequiresAtom(node, _) => Some(*node),
            OperatorError::UsedOnList(node, _) => Some(*node),
            OperatorError::TakesExactlyArgs(node, _, _) => Some(*node),
            OperatorError::ExpectedAtomGotPair(node) => Some(*node),
            OperatorError::InvalidArgs2or3(node, _) => Some(*node),
            OperatorError::InvalidIndices(node) => Some(*node),
            OperatorError::ConcatOnList(node) => Some(*node),
            OperatorError::NotValidG1Point(node) => Some(*node),
            OperatorError::G1MapInvalidArgs(node, _) => Some(*node),
            OperatorError::NotValidG2Point(node) => Some(*node),
            OperatorError::G2MapInvalidArgs(node, _) => Some(*node),
            OperatorError::NotG2Size(node) => Some(*node),
            OperatorError::BLSPairingIdentityFailed(node) => Some(*node),
            OperatorError::BLSVerifyFailed(node) => Some(*node),
            OperatorError::CoinID(coin_id_error) => CoinIDError::node(coin_id_error),
            OperatorError::Secp256k1Verify(secp_error) => Secp256k1verifyError::node(secp_error),
            OperatorError::Secp256r1Verify(secp_error) => Secp256r1verifyError::node(secp_error),
        }
    }
}

#[derive(Debug, Error)]
pub enum Secp256k1verifyError {
    #[error("failed")]
    Failed(NodePtr),

    #[error("pubkey is not valid")]
    PubkeyNotValid(NodePtr),

    #[error("message digest is not 32 bytes")]
    MessageDigestNot32Bytes(NodePtr),

    #[error("signature is not valid")]
    SignatureNotValid(NodePtr),
}
impl Secp256k1verifyError {
    pub fn node(&self) -> Option<NodePtr> {
        match self {
            Secp256k1verifyError::Failed(node) => Some(*node),
            Secp256k1verifyError::PubkeyNotValid(node) => Some(*node),
            Secp256k1verifyError::MessageDigestNot32Bytes(node) => Some(*node),
            Secp256k1verifyError::SignatureNotValid(node) => Some(*node),
        }
    }
}

#[derive(Debug, Error)]
pub enum Secp256r1verifyError {
    #[error("failed")]
    Failed(NodePtr),

    #[error("pubkey is not valid")]
    PubkeyNotValid(NodePtr),

    #[error("message digest is not 32 bytes")]
    MessageDigestNot32Bytes(NodePtr),

    #[error("signature is not valid")]
    SignatureNotValid(NodePtr),
}
impl Secp256r1verifyError {
    pub fn node(&self) -> Option<NodePtr> {
        match self {
            Secp256r1verifyError::Failed(node) => Some(*node),
            Secp256r1verifyError::PubkeyNotValid(node) => Some(*node),
            Secp256r1verifyError::MessageDigestNot32Bytes(node) => Some(*node),
            Secp256r1verifyError::SignatureNotValid(node) => Some(*node),
        }
    }
}

#[derive(Debug, Error)]
pub enum CoinIDError {
    #[error("Invalid Parent Coin ID, not 32 bytes")]
    ParentCoinIdNot32Bytes(NodePtr),

    #[error("Invalid Puzzle Hash, not 32 bytes")]
    PuzzleHashNot32Bytes(NodePtr),

    #[error("Invalid Amount: Amount is Negative")]
    AmountNegative(NodePtr),

    #[error("Invalid Amount: Amount has leading zeroes")]
    AmountLeadingZeroes(NodePtr),

    #[error("Invalid Amount: Amount exceeds max coin amount")]
    AmountExceedsMaxCoinAmount(NodePtr),
}
impl CoinIDError {
    pub fn node(&self) -> Option<NodePtr> {
        match self {
            CoinIDError::ParentCoinIdNot32Bytes(node) => Some(*node),
            CoinIDError::PuzzleHashNot32Bytes(node) => Some(*node),
            CoinIDError::AmountNegative(node) => Some(*node),
            CoinIDError::AmountLeadingZeroes(node) => Some(*node),
            CoinIDError::AmountExceedsMaxCoinAmount(node) => Some(*node),
        }
    }
}

// Allocator Errors
#[derive(Debug, Error)]
pub enum AllocatorError {
    #[error("Expected Atom, got Pair")]
    ExpectedAtomGotPair(NodePtr),

    #[error("Substring Start Index Out of Bounds: {1} > {2}")]
    StartOutOfBounds(NodePtr, u32, u32),

    #[error("Substring End Index Out of Bounds: {1} > {2}")]
    EndOutOfBounds(NodePtr, u32, u32),

    #[error("Substring Start Index Greater Than End Index: {2} < {1}")]
    StartGreaterThanEnd(NodePtr, u32, u32),

    #[error("concat passed invalid new_size: {1}")]
    InvalidNewSize(NodePtr, u32),

    #[error("atom is not G1 size (48 bytes)")]
    NotG1Size(NodePtr),

    #[error("pair found, expected G1 point")]
    ExpectedG1Point(NodePtr),

    #[error("atom is not a valid G1 point")]
    NotValidG1Point(NodePtr),

    #[error("atom is not G2 size (96 bytes)")]
    NotG2Size(NodePtr),

    #[error("pair found, expected G2 point")]
    ExpectedG2Point(NodePtr),

    #[error("atom is not a valid G2 point")]
    NotValidG2Point(NodePtr),
}

impl AllocatorError {
    pub fn node(&self) -> Option<NodePtr> {
        match self {
            AllocatorError::ExpectedAtomGotPair(node) => Some(*node),
            AllocatorError::StartOutOfBounds(node, _, _) => Some(*node),
            AllocatorError::EndOutOfBounds(node, _, _) => Some(*node),
            AllocatorError::StartGreaterThanEnd(node, _, _) => Some(*node),
            AllocatorError::InvalidNewSize(node, _) => Some(*node),
            AllocatorError::NotG1Size(node) => Some(*node),
            AllocatorError::ExpectedG1Point(node) => Some(*node),
            AllocatorError::NotValidG1Point(node) => Some(*node),
            AllocatorError::NotG2Size(node) => Some(*node),
            AllocatorError::ExpectedG2Point(node) => Some(*node),
            AllocatorError::NotValidG2Point(node) => Some(*node),
        }
    }
}
