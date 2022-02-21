use crate::allocator::{Allocator, NodePtr, SExp};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorCode {
    NegativeAmount,
    InvalidConditionOpcode,
    InvalidParentId,
    InvalidPuzzleHash,
    InvalidPubkey,
    InvalidMessage,
    InvalidCondition,
    InvalidCoinAmount,
    InvalidCoinAnnouncement,
    InvalidPuzzleAnnouncement,
    AssertHeightAbsolute,
    AssertHeightRelative,
    AssertSecondsAbsolute,
    AssertSecondsRelative,
    AssertMyAmountFailed,
    AssertMyPuzzlehashFailed,
    AssertMyParentIdFailed,
    AssertMyCoinIdFailed,
    AssertPuzzleAnnouncementFailed,
    AssertCoinAnnouncementFailed,
    ReserveFeeConditionFailed,
    DuplicateOutput,
    DoubleSpend,
    CostExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ValidationErr(pub NodePtr, pub ErrorCode);

// helper functions that fail with ValidationErr
pub fn first(a: &Allocator, n: NodePtr) -> Result<NodePtr, ValidationErr> {
    match a.sexp(n) {
        SExp::Pair(left, _) => Ok(left),
        _ => Err(ValidationErr(n, ErrorCode::InvalidCondition)),
    }
}

pub fn rest(a: &Allocator, n: NodePtr) -> Result<NodePtr, ValidationErr> {
    match a.sexp(n) {
        SExp::Pair(_, right) => Ok(right),
        _ => Err(ValidationErr(n, ErrorCode::InvalidCondition)),
    }
}

pub fn next(a: &Allocator, n: NodePtr) -> Result<Option<(NodePtr, NodePtr)>, ValidationErr> {
    match a.sexp(n) {
        SExp::Pair(left, right) => Ok(Some((left, right))),
        SExp::Atom(v) => {
            // this is expected to be a valid list terminator
            if v.is_empty() {
                Ok(None)
            } else {
                Err(ValidationErr(n, ErrorCode::InvalidCondition))
            }
        }
    }
}

pub fn atom(a: &Allocator, n: NodePtr, code: ErrorCode) -> Result<&[u8], ValidationErr> {
    match a.sexp(n) {
        SExp::Atom(_) => Ok(a.atom(n)),
        _ => Err(ValidationErr(n, code)),
    }
}

pub fn check_nil(a: &Allocator, n: NodePtr) -> Result<(), ValidationErr> {
    if atom(a, n, ErrorCode::InvalidCondition)?.is_empty() {
        Ok(())
    } else {
        Err(ValidationErr(n, ErrorCode::InvalidCondition))
    }
}

// from chia-blockchain/chia/util/errors.py
impl From<ErrorCode> for u16 {
    fn from(err: ErrorCode) -> u16 {
        match err {
            ErrorCode::NegativeAmount => 124,
            ErrorCode::InvalidPuzzleHash => 10,
            ErrorCode::InvalidPubkey => 10,
            ErrorCode::InvalidMessage => 10,
            ErrorCode::InvalidParentId => 10,
            ErrorCode::InvalidConditionOpcode => 10,
            ErrorCode::InvalidCoinAnnouncement => 10,
            ErrorCode::InvalidPuzzleAnnouncement => 10,
            ErrorCode::InvalidCondition => 10,
            ErrorCode::InvalidCoinAmount => 10,
            ErrorCode::AssertHeightAbsolute => 14,
            ErrorCode::AssertHeightRelative => 13,
            ErrorCode::AssertSecondsAbsolute => 15,
            ErrorCode::AssertSecondsRelative => 105,
            ErrorCode::AssertMyAmountFailed => 116,
            ErrorCode::AssertMyPuzzlehashFailed => 115,
            ErrorCode::AssertMyParentIdFailed => 114,
            ErrorCode::AssertMyCoinIdFailed => 11,
            ErrorCode::AssertPuzzleAnnouncementFailed => 12,
            ErrorCode::AssertCoinAnnouncementFailed => 12,
            ErrorCode::ReserveFeeConditionFailed => 48,
            ErrorCode::DuplicateOutput => 4,
            ErrorCode::DoubleSpend => 5,
            ErrorCode::CostExceeded => 23,
        }
    }
}
