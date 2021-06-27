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

pub fn checked_add(lhs: u64, rhs: u64, n: NodePtr, e: ErrorCode) -> Result<u64, ValidationErr> {
    if u64::MAX - lhs < rhs {
        Err(ValidationErr(n, e))
    } else {
        Ok(lhs + rhs)
    }
}

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

pub fn pair(a: &Allocator, n: NodePtr) -> Option<(NodePtr, NodePtr)> {
    match a.sexp(n) {
        SExp::Pair(left, right) => Some((left, right)),
        _ => None,
    }
}

pub fn next(a: &Allocator, n: NodePtr) -> Result<(NodePtr, NodePtr), ValidationErr> {
    match a.sexp(n) {
        SExp::Pair(left, right) => Ok((left, right)),
        _ => Err(ValidationErr(n, ErrorCode::InvalidCondition)),
    }
}

pub fn atom(a: &Allocator, n: NodePtr, code: ErrorCode) -> Result<&[u8], ValidationErr> {
    match a.sexp(n) {
        SExp::Atom(_) => Ok(a.atom(n)),
        _ => Err(ValidationErr(n, code)),
    }
}

#[test]
fn test_checked_add() {
    let a = Allocator::new();
    assert_eq!(
        checked_add(u64::MAX - 1, 1, a.null(), ErrorCode::CostExceeded),
        Ok(u64::MAX)
    );
    assert_eq!(checked_add(1, 1, a.null(), ErrorCode::CostExceeded), Ok(2));
    assert_eq!(
        checked_add(1024, 1024, a.null(), ErrorCode::CostExceeded),
        Ok(2048)
    );

    assert_eq!(
        checked_add(u64::MAX, 1, a.null(), ErrorCode::CostExceeded),
        Err(ValidationErr(a.null(), ErrorCode::CostExceeded))
    );
    assert_eq!(
        checked_add(u64::MAX, u64::MAX, a.null(), ErrorCode::CostExceeded),
        Err(ValidationErr(a.null(), ErrorCode::CostExceeded))
    );
}
