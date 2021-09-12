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
            // this is a valid list terminator
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
