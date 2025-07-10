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

    #[error("InvalidArg: {1}")]
    InvalidArg(NodePtr, String),

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
    
    #[error("Operator Error: Reserved operator")]
    Reserved(NodePtr),

    #[error("Operator Error: Invalid Operator")]
    Invalid(NodePtr),

    #[error("Operator Error: Unimplemented Operator")]
    Unimplemented(NodePtr),

    #[error("Operator Error: Requires Int Argument: {1}")]
    RequiresIntArgument(NodePtr, String),

    #[error("Operator Error: {1} Requires Positive Int Argument")]
    RequiresPositiveIntArgument(NodePtr, String),

    #[error("Operator Error: {1} Requires Int32 args (with no leading zeros)")]
    RequiresInt32Args(NodePtr, String),

    #[error("Operator Error: {1} Requires Int64 args (with no leading zeros)")]
    RequiresInt64Args(NodePtr, String),

    #[error("Operator Error: {1} Requires {2} arguments")]
    RequiresArgs(NodePtr, String, u32),

    #[error("Operator Error: {1} takes no more then {2} arguments")]
    TakesNoMoreThanArgs(NodePtr, String, u32),

    #[error("Operator Error: {1} requires an atom")]
    RequiresAtom(NodePtr, String),

    #[error("Operator Error: {1} used on list")]
    UsedOnList(NodePtr, String),

    #[error("Operator Error: {1} takes exactly {2} argument(s)")]
    TakesExactlyArgs(NodePtr, String, u32),

    #[error("Operator Error: Expected Atom, got Pair")]
    ExpectedAtomGotPair(NodePtr),

    #[error("Operator Error: Substring takes exactly 2 or 3 arguments, got {1}")]
    InvalidArgs2or3(NodePtr, u32),

    #[error("Operator Error: Invalid Indices for Substring")]
    InvalidIndices(NodePtr),

    #[error("Operator Error: concat on list")]
    ConcatOnList(NodePtr),

    #[error("Operator Error: atom is not a valid G1 point")]
    NotValidG1Point(NodePtr),

    #[error("Operator Error: G1_map takes exactly 1 or 2 arguments, got {1}")]
    G1MapInvalidArgs(NodePtr, u32),

    #[error("Operator Error: atom is not a valid G2 point")]
    NotValidG2Point(NodePtr),

    #[error("Operator Error: G2_map takes exactly 1 or 2 arguments, got {1}")]
    G2MapInvalidArgs(NodePtr, u32),

    #[error("Operator Error: atom is not G2 size (96 bytes)")]
    NotG2Size(NodePtr),

    #[error("Operator Error: bls_pairing_identity failed")]
    BLSPairingIdentityFailed(NodePtr),

    #[error("Operator Error: bls_verify failed")]
    BLSVerifyFailed(NodePtr),

    #[error("Operator Error: Secp256k1 Verify Error: failed")]
    Secp256k1Failed(NodePtr),

    #[error("Operator Error: Secp256k1 Verify Error: pubkey is not valid")]
    Secp256k1PubkeyNotValid(NodePtr),

    #[error("Operator Error: Secp256k1 Verify Error: message digest is not 32 bytes")]
    Secp256k1MessageDigestNot32Bytes(NodePtr),

    #[error("Operator Error: Secp256k1 Verify Error: signature is not valid")]
    Secp256k1SignatureNotValid(NodePtr),

    #[error("Operator Error: Secp256r1 Verify Error: failed")]
    Secp256r1Failed(NodePtr),

    #[error("Operator Error: Secp256r1 Verify Error: pubkey is not valid")]
    Secp256r1PubkeyNotValid(NodePtr),

    #[error("Operator Error: Secp256r1 Verify Error: message digest is not 32 bytes")]
    Secp256r1MessageDigestNot32Bytes(NodePtr),

    #[error("Operator Error: Secp256r1 Verify Error: signature is not valid")]
    Secp256r1SignatureNotValid(NodePtr),

    #[error("Operator Error: CoinID Error: Invalid Parent Coin ID, not 32 bytes")]
    CoinIDParentCoinIdNot32Bytes(NodePtr),

    #[error("Operator Error: CoinID Error: Invalid Puzzle Hash, not 32 bytes")]
    CoinIDPuzzleHashNot32Bytes(NodePtr),

    #[error("Operator Error: CoinID Error: Invalid Amount: Amount is Negative")]
    CoinIDAmountNegative(NodePtr),

    #[error("Operator Error: CoinID Error: Invalid Amount: Amount has leading zeroes")]
    CoinIDAmountLeadingZeroes(NodePtr),

    #[error("Operator Error: CoinID Error: Invalid Amount: Amount exceeds max coin amount")]
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
            EvalErr::Reserved(node) => Some(*node),
            EvalErr::Invalid(node) => Some(*node),
            EvalErr::Unimplemented(node) => Some(*node),
            EvalErr::RequiresIntArgument(node, _) => Some(*node),
            EvalErr::RequiresPositiveIntArgument(node, _) => Some(*node),
            EvalErr::RequiresInt32Args(node, _) => Some(*node),
            EvalErr::RequiresInt64Args(node, _) => Some(*node),
            EvalErr::RequiresArgs(node, _, _) => Some(*node),
            EvalErr::TakesNoMoreThanArgs(node, _, _) => Some(*node),
            EvalErr::RequiresAtom(node, _) => Some(*node),
            EvalErr::UsedOnList(node, _) => Some(*node),
            EvalErr::TakesExactlyArgs(node, _, _) => Some(*node),
            EvalErr::ExpectedAtomGotPair(node) => Some(*node),
            EvalErr::InvalidArgs2or3(node, _) => Some(*node),
            EvalErr::InvalidIndices(node) => Some(*node),
            EvalErr::ConcatOnList(node) => Some(*node),
            EvalErr::NotValidG1Point(node) => Some(*node),
            EvalErr::G1MapInvalidArgs(node, _) => Some(*node),
            EvalErr::NotValidG2Point(node) => Some(*node),
            EvalErr::G2MapInvalidArgs(node, _) => Some(*node),
            EvalErr::NotG2Size(node) => Some(*node),
            EvalErr::BLSPairingIdentityFailed(node) => Some(*node),
            EvalErr::BLSVerifyFailed(node) => Some(*node),
            EvalErr::Secp256k1Failed(node) => Some(*node),
            EvalErr::Secp256k1PubkeyNotValid(node) => Some(*node),
            EvalErr::Secp256k1MessageDigestNot32Bytes(node) => Some(*node),
            EvalErr::Secp256k1SignatureNotValid(node) => Some(*node),
            EvalErr::Secp256r1Failed(node) => Some(*node),
            EvalErr::Secp256r1PubkeyNotValid(node) => Some(*node),
            EvalErr::Secp256r1MessageDigestNot32Bytes(node) => Some(*node),
            EvalErr::Secp256r1SignatureNotValid(node) => Some(*node),
            EvalErr::CoinIDPuzzleHashNot32Bytes(node) => Some(*node),
            EvalErr::CoinIDAmountNegative(node) => Some(*node),
            EvalErr::CoinIDAmountLeadingZeroes(node) => Some(*node),
            EvalErr::CoinIDAmountExceedsMaxCoinAmount(node) => Some(*node),
            EvalErr::CoinIDParentCoinIdNot32Bytes(node) => Some(*node),
            EvalErr::Allocator(alloc) => AllocatorErr::node(alloc),
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
    #[error("Expected Atom, got Pair")]
    ExpectedAtomGotPair(NodePtr),

    #[error("InvalidArg: {1}")]
    InvalidArg(NodePtr, String),

}

impl AllocatorErr {
    pub fn node(&self) -> Option<NodePtr> {
        match self {
            AllocatorErr::InvalidArg(node, _) => Some(*node),
            AllocatorErr::ExpectedAtomGotPair(node) => Some(*node),
        }
    }
}
