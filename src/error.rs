use crate::{Allocator, NodePtr, ObjectType};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, EvalErr>;
#[derive(Debug, Error)]
pub enum EvalErr {
    #[error("Internal Error: {0}")]
    InternalError(String),

    #[error("Encoding / Decoding Error")]
    SerializationError,
    #[error("clvm raise, {0:?}")]
    Raise(NodePtr),

    #[error("Out of Memory")]
    OutOfMemory,

    #[error("Cost Exceeded")]
    CostExceeded,

    #[error("Cost Must be greater than zero")]
    CostBelowZero,

    #[error("Too Many Pairs")]
    TooManyPairs,

    #[error("Too Many Atoms")]
    TooManyAtoms,

    #[error("Path Into Atom {0:?}")]
    PathIntoAtom(NodePtr),

    #[error("in ((X)...) syntax X must be lone atom")]
    InPairMustBeLoneAtom(NodePtr),

    #[error("Invalid Nil Terminator: {0:?}")]
    InvalidNilTerminator(NodePtr),

    #[error("First of non-cons: {0:?}")]
    FirstOfNonCons(NodePtr),

    #[error("Rest of non-cons: {0:?}")]
    RestOfNonCons(NodePtr),

    #[error("Division by zero: {0:?}")]
    DivisionByZero(NodePtr),

    #[error("Divmod by zero: {0:?}")]
    DivmodByZero(NodePtr),

    #[error("Mod by zero: {0:?}")]
    ModByZero(NodePtr),

    #[error("ModPow with Negative Exponent: {0:?}")]
    ModPowNegativeExponent(NodePtr),

    #[error("ModPow with 0 Modulus: {0:?}")]
    ModPowZeroModulus(NodePtr),

    #[error("Shift too large: {0:?}")]
    ShiftTooLarge(NodePtr),

    #[error("Unknown Softfork Extension: {0:?}")]
    UnknownSoftforkExtension(NodePtr),

    #[error("Softfork specified cost mismatch")]
    SoftforkSpecifiedCostMismatch,

    #[error("Value Stack Limit Reached, {0:?}")]
    ValueStackLimitReached(NodePtr),

    #[error("Environment Stack Limit Reached, {0:?}")]
    EnvironmentStackLimitReached(NodePtr),

    // Grouped errors
    #[error("Operator Error: {0}")]
    Operator(#[from] OperatorError),

    #[error("Allocator Error: {0}")]
    Allocator(#[from] AllocatorError),
}

impl PartialEq<Self> for EvalErr {
    fn eq(&self, other: &Self) -> bool {
        self.to_string() == other.to_string()
    }
}
// Operator Errors
#[derive(Debug, Error)]
pub enum OperatorError {
    #[error("Reserved operator: {0:?}")]
    Reserved(NodePtr),

    #[error("Invalid Operator: {0:?}")]
    Invalid(NodePtr),

    #[error("Unimplemented Operator {0:?}")]
    Unimplemented(NodePtr),

    #[error("Requires Int Argument: {1}, {0:?}")]
    RequiresIntArgument(NodePtr, String),

    #[error("{1} Requires Positive Int Argument, {0:?}")]
    RequiresPositiveIntArgument(NodePtr, String),

    #[error("{1} Requires Int32 args (with no leading zeros), {0:?}")]
    RequiresInt32Args(NodePtr, String),

    #[error("{1} Requires {2} arguments, {0:?}")]
    RequiresArgs(NodePtr, String, u32),

    #[error("{1} takes no more then {2} arguments, {0:?}")]
    TakesNoMoreThanArgs(NodePtr, String, u32),

    #[error("{1} requires an atom: {0:?}")]
    RequiresAtom(NodePtr, String),

    #[error("{1} used on list, {0:?}")]
    UsedOnList(NodePtr, String),

    #[error("{1} takes exactly {2} argument(s), {0:?}")]
    TakesExactlyArgs(NodePtr, String, u32),

    #[error("Expected Atom, got Pair: {0:?}")]
    ExpectedAtomGotPair(NodePtr),

    #[error("Substring takes exactly 2 or 3 arguments, got {1}, {0:?}")]
    InvalidArgs2or3(NodePtr, u32),

    #[error("Invalid Indices for Substring: {0:?}")]
    InvalidIndices(NodePtr),

    #[error("concat on list, {0:?}")]
    ConcatOnList(NodePtr),

    #[error("atom is not a valid G1 point, {0:?}")]
    NotValidG1Point(NodePtr),

    #[error("G1_map takes exactly 1 or 2 arguments, got {1}, {0:?}")]
    G1MapInvalidArgs(NodePtr, u32),

    #[error("atom is not a valid G2 point, {0:?}")]
    NotValidG2Point(NodePtr),

    #[error("G2_map takes exactly 1 or 2 arguments, got {1}, {0:?}")]
    G2MapInvalidArgs(NodePtr, u32),

    #[error("atom is not G2 size (96 bytes), {0:?}")]
    NotG2Size(NodePtr),

    #[error("bls_pairing_identity failed, {0:?}")]
    BLSPairingIdentityFailed(NodePtr),

    #[error("bls_verify failed, {0:?}")]
    BLSVerifyFailed(NodePtr),

    #[error("CoinID Error: {0:?}")]
    CoinID(#[from] CoinIDError),

    #[error("Secp256k1 Verify Error: {0}")]
    Secp256k1Verify(#[from] Secp256k1verifyError),

    #[error("Secp256r1 Verify Error: {0}")]
    Secp256r1Verify(#[from] Secp256r1verifyError),
}

#[derive(Debug, Error)]
pub enum Secp256k1verifyError {
    #[error("failed, {0:?}")]
    Failed(NodePtr),
    #[error("pubkey is not valid, {0:?}")]
    PubkeyNotValid(NodePtr),
    #[error("message digest is not 32 bytes, {0:?}")]
    MessageDigestNot32Bytes(NodePtr),
    #[error("signature is not valid, {0:?}")]
    SignatureNotValid(NodePtr),
}

#[derive(Debug, Error)]
pub enum Secp256r1verifyError {
    #[error("failed, {0:?}")]
    Failed(NodePtr),
    #[error("pubkey is not valid, {0:?}")]
    PubkeyNotValid(NodePtr),
    #[error("message digest is not 32 bytes, {0:?}")]
    MessageDigestNot32Bytes(NodePtr),
    #[error("signature is not valid, {0:?}")]
    SignatureNotValid(NodePtr),
}

#[derive(Debug, Error)]
pub enum CoinIDError {
    #[error("Invalid Parent Coin ID, not 32 bytes, {0:?}")]
    ParentCoinIdNot32Bytes(NodePtr),

    #[error("Invalid Puzzle Hash, not 32 bytes, {0:?}")]
    PuzzleHashNot32Bytes(NodePtr),

    #[error("Invalid Amount: Amount is Negative, {0:?}")]
    AmountNegative(NodePtr),

    #[error("Invalid Amount: Amount has leading zeroes, {0:?}")]
    AmountLeadingZeroes(NodePtr),

    #[error("Invalid Amount: Amount exceeds max coin amount, {0:?}")]
    AmountExceedsMaxCoinAmount(NodePtr),
}

// Allocator Errors
#[derive(Debug, Error)]
pub enum AllocatorError {
    #[error("Expected Atom, got Pair: {0:?}")]
    ExpectedAtomGotPair(NodePtr),

    #[error("Substring Start Index Out of Bounds: {1} > {2}, {0:?}")]
    StartOutOfBounds(NodePtr, u32, u32),

    #[error("Substring End Index Out of Bounds: {1} > {2}, {0:?}")]
    EndOutOfBounds(NodePtr, u32, u32),

    #[error("Substring Start Index Greater Than End Index: {2} < {1}, {0:?}")]
    StartGreaterThanEnd(NodePtr, u32, u32),

    #[error("concat passed invalid new_size: {1}, {0:?}")]
    InvalidNewSize(NodePtr, u32),

    #[error("atom is not G1 size (48 bytes), {0:?}")]
    NotG1Size(NodePtr),

    #[error("pair found, expected G1 point, {0:?}")]
    ExpectedG1Point(NodePtr),

    #[error("atom is not a valid G1 point, {0:?}")]
    NotValidG1Point(NodePtr),

    #[error("atom is not G2 size (96 bytes), {0:?}")]
    NotG2Size(NodePtr),

    #[error("pair found, expected G2 point, {0:?}")]
    ExpectedG2Point(NodePtr),

    #[error("atom is not a valid G2 point, {0:?}")]
    NotValidG2Point(NodePtr),
}

// Helper Functions for Debugging

pub fn h_byte_false(allocator: &Allocator) -> NodePtr {
    allocator.mk_node(ObjectType::Bytes, 0)
}

pub fn h_byte_true(allocator: &Allocator) -> NodePtr {
    allocator.mk_node(ObjectType::Bytes, 1)
}

pub fn h_pair(allocator: &Allocator) -> NodePtr {
    allocator.mk_node(ObjectType::Pair, 0)
}
