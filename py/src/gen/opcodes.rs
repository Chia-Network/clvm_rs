use clvm_rs::allocator::{Allocator, NodePtr, SExp};
use clvm_rs::cost::Cost;

pub type ConditionOpcode = u8;

// AGG_SIG is ascii "1"
pub const AGG_SIG_UNSAFE: ConditionOpcode = 49;
pub const AGG_SIG_ME: ConditionOpcode = 50;

// the conditions below reserve coin amounts and have to be accounted for in
// output totals
pub const CREATE_COIN: ConditionOpcode = 51;
pub const RESERVE_FEE: ConditionOpcode = 52;

// the conditions below deal with announcements, for inter-coin communication
pub const CREATE_COIN_ANNOUNCEMENT: ConditionOpcode = 60;
pub const ASSERT_COIN_ANNOUNCEMENT: ConditionOpcode = 61;
pub const CREATE_PUZZLE_ANNOUNCEMENT: ConditionOpcode = 62;
pub const ASSERT_PUZZLE_ANNOUNCEMENT: ConditionOpcode = 63;

// the conditions below let coins inquire about themselves
pub const ASSERT_MY_COIN_ID: ConditionOpcode = 70;
pub const ASSERT_MY_PARENT_ID: ConditionOpcode = 71;
pub const ASSERT_MY_PUZZLEHASH: ConditionOpcode = 72;
pub const ASSERT_MY_AMOUNT: ConditionOpcode = 73;

// the conditions below ensure that we're "far enough" in the future
// wall-clock time
pub const ASSERT_SECONDS_RELATIVE: ConditionOpcode = 80;
pub const ASSERT_SECONDS_ABSOLUTE: ConditionOpcode = 81;

// block index
pub const ASSERT_HEIGHT_RELATIVE: ConditionOpcode = 82;
pub const ASSERT_HEIGHT_ABSOLUTE: ConditionOpcode = 83;

pub const CREATE_COIN_COST: Cost = 1800000;
pub const AGG_SIG_COST: Cost = 1200000;

pub fn parse_opcode(a: &Allocator, op: NodePtr) -> Option<ConditionOpcode> {
    let buf = match a.sexp(op) {
        SExp::Atom(_) => a.atom(op),
        _ => return None,
    };
    if buf.len() != 1 {
        return None;
    }
    match buf[0] {
        AGG_SIG_UNSAFE
        | AGG_SIG_ME
        | CREATE_COIN
        | RESERVE_FEE
        | CREATE_COIN_ANNOUNCEMENT
        | ASSERT_COIN_ANNOUNCEMENT
        | CREATE_PUZZLE_ANNOUNCEMENT
        | ASSERT_PUZZLE_ANNOUNCEMENT
        | ASSERT_MY_COIN_ID
        | ASSERT_MY_PARENT_ID
        | ASSERT_MY_PUZZLEHASH
        | ASSERT_MY_AMOUNT
        | ASSERT_SECONDS_RELATIVE
        | ASSERT_SECONDS_ABSOLUTE
        | ASSERT_HEIGHT_RELATIVE
        | ASSERT_HEIGHT_ABSOLUTE => Some(buf[0]),
        _ => None,
    }
}

#[cfg(test)]
fn opcode_tester(a: &mut Allocator, val: &[u8]) -> Option<ConditionOpcode> {
    let v = a.new_atom(val).unwrap();
    parse_opcode(&a, v)
}

#[test]
fn test_parse_opcode() {
    let mut a = Allocator::new();
    assert_eq!(
        opcode_tester(&mut a, &[AGG_SIG_UNSAFE]),
        Some(AGG_SIG_UNSAFE)
    );
    assert_eq!(opcode_tester(&mut a, &[AGG_SIG_ME]), Some(AGG_SIG_ME));
    assert_eq!(opcode_tester(&mut a, &[CREATE_COIN]), Some(CREATE_COIN));
    assert_eq!(opcode_tester(&mut a, &[RESERVE_FEE]), Some(RESERVE_FEE));
    assert_eq!(
        opcode_tester(&mut a, &[CREATE_COIN_ANNOUNCEMENT]),
        Some(CREATE_COIN_ANNOUNCEMENT)
    );
    assert_eq!(
        opcode_tester(&mut a, &[ASSERT_COIN_ANNOUNCEMENT]),
        Some(ASSERT_COIN_ANNOUNCEMENT)
    );
    assert_eq!(
        opcode_tester(&mut a, &[CREATE_PUZZLE_ANNOUNCEMENT]),
        Some(CREATE_PUZZLE_ANNOUNCEMENT)
    );
    assert_eq!(
        opcode_tester(&mut a, &[ASSERT_PUZZLE_ANNOUNCEMENT]),
        Some(ASSERT_PUZZLE_ANNOUNCEMENT)
    );
    assert_eq!(
        opcode_tester(&mut a, &[ASSERT_MY_COIN_ID]),
        Some(ASSERT_MY_COIN_ID)
    );
    assert_eq!(
        opcode_tester(&mut a, &[ASSERT_MY_PARENT_ID]),
        Some(ASSERT_MY_PARENT_ID)
    );
    assert_eq!(
        opcode_tester(&mut a, &[ASSERT_MY_PUZZLEHASH]),
        Some(ASSERT_MY_PUZZLEHASH)
    );
    assert_eq!(
        opcode_tester(&mut a, &[ASSERT_MY_AMOUNT]),
        Some(ASSERT_MY_AMOUNT)
    );
    assert_eq!(
        opcode_tester(&mut a, &[ASSERT_SECONDS_RELATIVE]),
        Some(ASSERT_SECONDS_RELATIVE)
    );
    assert_eq!(
        opcode_tester(&mut a, &[ASSERT_SECONDS_ABSOLUTE]),
        Some(ASSERT_SECONDS_ABSOLUTE)
    );
    assert_eq!(
        opcode_tester(&mut a, &[ASSERT_HEIGHT_RELATIVE]),
        Some(ASSERT_HEIGHT_RELATIVE)
    );
    assert_eq!(
        opcode_tester(&mut a, &[ASSERT_HEIGHT_ABSOLUTE]),
        Some(ASSERT_HEIGHT_ABSOLUTE)
    );
    // leading zeros are not allowed, it makes it a different value
    assert_eq!(opcode_tester(&mut a, &[ASSERT_HEIGHT_ABSOLUTE, 0]), None);
    assert_eq!(opcode_tester(&mut a, &[0, ASSERT_HEIGHT_ABSOLUTE]), None);
    assert_eq!(opcode_tester(&mut a, &[0]), None);

    // a pair is never a valid condition
    let v1 = a.new_atom(&[0]).unwrap();
    let v2 = a.new_atom(&[0]).unwrap();
    let p = a.new_pair(v1, v2).unwrap();
    assert_eq!(parse_opcode(&a, p), None);
}
