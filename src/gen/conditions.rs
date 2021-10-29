use super::coin_id::compute_coin_id;
use super::condition_sanitizers::{
    parse_amount, parse_height, parse_seconds, sanitize_announce_msg, sanitize_hash,
};
use super::rangeset::RangeSet;
use super::sanitize_int::sanitize_uint;
use super::validation_error::{first, next, rest, ErrorCode, ValidationErr};
use crate::allocator::{Allocator, NodePtr, SExp};
use crate::cost::Cost;
use crate::gen::opcodes::{
    parse_opcode, ConditionOpcode, AGG_SIG_COST, AGG_SIG_ME, AGG_SIG_UNSAFE,
    ASSERT_COIN_ANNOUNCEMENT, ASSERT_HEIGHT_ABSOLUTE, ASSERT_HEIGHT_RELATIVE, ASSERT_MY_AMOUNT,
    ASSERT_MY_COIN_ID, ASSERT_MY_PARENT_ID, ASSERT_MY_PUZZLEHASH, ASSERT_PUZZLE_ANNOUNCEMENT,
    ASSERT_SECONDS_ABSOLUTE, ASSERT_SECONDS_RELATIVE, CREATE_COIN, CREATE_COIN_ANNOUNCEMENT,
    CREATE_COIN_COST, CREATE_PUZZLE_ANNOUNCEMENT, RESERVE_FEE,
};
use crate::op_utils::u64_from_bytes;
use crate::run_program::STRICT_MODE;
use crate::sha2::Sha256;
use std::cmp::max;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

// The structure of conditions, returned from a generator program, is a list,
// where the first element is used, and any additional elements are left unused,
// for future soft-forks.

// The first element is, in turn, a list of all coins being spent.

// Each spend has the following structure:

// (<coin-parent-id> <coin-puzzle-hash> <coin-amount> (CONDITION-LIST ...) ... )

// where ... is possible extra fields that are currently ignored.

// the CONDITIONS-LIST, lists all the conditions for the spend, including
// CREATE_COINs. It has the following format:

// (<condition-opcode> <arg1> <arg2> ...)

// different conditions have different number and types of arguments.

// Example:

// ((<coin-parent-id> <coind-puzzle-hash> <coin-amount> (
//  (
//    (CREATE_COIN <puzzle-hash> <amount>)
//    (ASSERT_HEIGHT_ABSOLUTE <height>)
//  )
// )))

#[derive(PartialEq, Hash, Eq, Debug)]
pub enum Condition {
    // pubkey (48 bytes) and message (<= 1024 bytes)
    AggSigUnsafe(NodePtr, NodePtr),
    AggSigMe(NodePtr, NodePtr),
    // puzzle hash (32 bytes), amount-node, amount integer, hint is an optional
    // hash (32 bytes), may be left as null
    CreateCoin(NodePtr, u64, NodePtr),
    // amount
    ReserveFee(u64),
    // message (<= 1024 bytes)
    CreateCoinAnnouncement(NodePtr),
    CreatePuzzleAnnouncement(NodePtr),
    // announce ID (hash, 32 bytes)
    AssertCoinAnnouncement(NodePtr),
    AssertPuzzleAnnouncement(NodePtr),
    // ID (hash, 32 bytes)
    AssertMyCoinId(NodePtr),
    AssertMyParentId(NodePtr),
    AssertMyPuzzlehash(NodePtr),
    // amount
    AssertMyAmount(u64),
    // seconds
    AssertSecondsRelative(u64),
    AssertSecondsAbsolute(u64),
    // block height
    AssertHeightRelative(u32),
    AssertHeightAbsolute(u32),

    // this means the condition is unconditionally true and can be skipped
    Skip,
}

fn parse_args(
    a: &Allocator,
    mut c: NodePtr,
    op: ConditionOpcode,
    range_cache: &mut RangeSet,
    flags: u32,
) -> Result<Condition, ValidationErr> {
    match op {
        AGG_SIG_UNSAFE => {
            let pubkey = sanitize_hash(a, first(a, c)?, 48, ErrorCode::InvalidPubkey)?;
            c = rest(a, c)?;
            let message = sanitize_announce_msg(a, first(a, c)?, ErrorCode::InvalidMessage)?;
            // AGG_SIG_UNSAFE takes exactly two parameters
            // but the argument list still doesn't need to be terminated by NIL,
            // just any atom will do
            match a.sexp(rest(a, c)?) {
                SExp::Pair(_, _) => Err(ValidationErr(c, ErrorCode::InvalidCondition)),
                _ => Ok(Condition::AggSigUnsafe(pubkey, message)),
            }
        }
        AGG_SIG_ME => {
            let pubkey = sanitize_hash(a, first(a, c)?, 48, ErrorCode::InvalidPubkey)?;
            c = rest(a, c)?;
            let message = sanitize_announce_msg(a, first(a, c)?, ErrorCode::InvalidMessage)?;
            // AGG_SIG_ME takes exactly two parameters
            // but the argument list still doesn't need to be terminated by NIL,
            // just any atom will do
            match a.sexp(rest(a, c)?) {
                SExp::Pair(_, _) => Err(ValidationErr(c, ErrorCode::InvalidCondition)),
                _ => Ok(Condition::AggSigMe(pubkey, message)),
            }
        }
        CREATE_COIN => {
            let puzzle_hash = sanitize_hash(a, first(a, c)?, 32, ErrorCode::InvalidPuzzleHash)?;
            c = rest(a, c)?;
            let amount = parse_amount(
                a,
                first(a, c)?,
                ErrorCode::InvalidCoinAmount,
                range_cache,
                flags,
            )?;
            // CREATE_COIN takes an optional 3rd parameter, which is a list of
            // byte buffers (typically a 32 byte hash). We only pull out the
            // first element for now, but may support more in the future.
            // If we find anything else, that's still OK, since garbage is
            // ignored.
            if let Ok(c) = rest(a, c) {
                // there was another item in the list
                if let Ok(params) = first(a, c) {
                    // the item was a cons-box, and params is the left-hand
                    // side, the list element
                    if let Ok(param) = first(a, params) {
                        // pull out the first item (param)
                        if let SExp::Atom(b) = a.sexp(param) {
                            if a.buf(&b).len() <= 32 {
                                return Ok(Condition::CreateCoin(puzzle_hash, amount, param));
                            }
                        }
                    }
                }
            }
            Ok(Condition::CreateCoin(puzzle_hash, amount, a.null()))
        }
        RESERVE_FEE => {
            let fee = parse_amount(
                a,
                first(a, c)?,
                ErrorCode::ReserveFeeConditionFailed,
                range_cache,
                flags,
            )?;
            Ok(Condition::ReserveFee(fee))
        }
        CREATE_COIN_ANNOUNCEMENT => {
            let msg = sanitize_announce_msg(a, first(a, c)?, ErrorCode::InvalidCoinAnnouncement)?;
            Ok(Condition::CreateCoinAnnouncement(msg))
        }
        ASSERT_COIN_ANNOUNCEMENT => {
            let id = sanitize_hash(a, first(a, c)?, 32, ErrorCode::AssertCoinAnnouncementFailed)?;
            Ok(Condition::AssertCoinAnnouncement(id))
        }
        CREATE_PUZZLE_ANNOUNCEMENT => {
            let msg = sanitize_announce_msg(a, first(a, c)?, ErrorCode::InvalidPuzzleAnnouncement)?;
            Ok(Condition::CreatePuzzleAnnouncement(msg))
        }
        ASSERT_PUZZLE_ANNOUNCEMENT => {
            let id = sanitize_hash(
                a,
                first(a, c)?,
                32,
                ErrorCode::AssertPuzzleAnnouncementFailed,
            )?;
            Ok(Condition::AssertPuzzleAnnouncement(id))
        }
        ASSERT_MY_COIN_ID => {
            let id = sanitize_hash(a, first(a, c)?, 32, ErrorCode::AssertMyCoinIdFailed)?;
            Ok(Condition::AssertMyCoinId(id))
        }
        ASSERT_MY_PARENT_ID => {
            let id = sanitize_hash(a, first(a, c)?, 32, ErrorCode::AssertMyParentIdFailed)?;
            Ok(Condition::AssertMyParentId(id))
        }
        ASSERT_MY_PUZZLEHASH => {
            let id = sanitize_hash(a, first(a, c)?, 32, ErrorCode::AssertMyPuzzlehashFailed)?;
            Ok(Condition::AssertMyPuzzlehash(id))
        }
        ASSERT_MY_AMOUNT => {
            let amount = parse_amount(
                a,
                first(a, c)?,
                ErrorCode::AssertMyAmountFailed,
                range_cache,
                flags,
            )?;
            Ok(Condition::AssertMyAmount(amount))
        }
        ASSERT_SECONDS_RELATIVE => Ok(Condition::AssertSecondsRelative(parse_seconds(
            a,
            first(a, c)?,
            ErrorCode::AssertSecondsRelative,
            range_cache,
            flags,
        )?)),
        ASSERT_SECONDS_ABSOLUTE => Ok(Condition::AssertSecondsAbsolute(parse_seconds(
            a,
            first(a, c)?,
            ErrorCode::AssertSecondsAbsolute,
            range_cache,
            flags,
        )?)),
        ASSERT_HEIGHT_RELATIVE => match sanitize_uint(
            a,
            first(a, c)?,
            4,
            ErrorCode::AssertHeightRelative,
            range_cache,
            flags,
        ) {
            // Height is always positive, so a negative requirement is always true,
            Err(ValidationErr(_, ErrorCode::NegativeAmount)) => Ok(Condition::Skip),
            Err(r) => Err(r),
            Ok(r) => Ok(Condition::AssertHeightRelative(u64_from_bytes(r) as u32)),
        },
        ASSERT_HEIGHT_ABSOLUTE => Ok(Condition::AssertHeightAbsolute(parse_height(
            a,
            first(a, c)?,
            ErrorCode::AssertHeightAbsolute,
            range_cache,
            flags,
        )?)),
        _ => Err(ValidationErr(c, ErrorCode::InvalidConditionOpcode)),
    }
}

#[derive(Debug)]
pub struct NewCoin {
    pub puzzle_hash: Vec<u8>,
    pub amount: u64,
    // the hint is optional. When not provided, this points to null (NodePtr
    // value -1). The hint is not part of the unique identity of a coin, it's not
    // hashed when computing the coin ID
    pub hint: NodePtr,
}

impl Hash for NewCoin {
    fn hash<H: Hasher>(&self, h: &mut H) {
        self.puzzle_hash.hash(h);
        self.amount.hash(h);
    }
}

impl Eq for NewCoin {}

impl PartialEq for NewCoin {
    fn eq(&self, lhs: &NewCoin) -> bool {
        self.amount == lhs.amount && self.puzzle_hash == lhs.puzzle_hash
    }
}

// These are all the conditions related directly to a specific spend.
#[derive(Debug)]
pub struct Spend {
    pub coin_id: Arc<[u8; 32]>,
    pub puzzle_hash: NodePtr,
    // conditions
    // all these integers are initialized to 0, which also means "no
    // constraint". i.e. a 0 in these conditions are inherently satisified and
    // ignored. 0 (or negative values) are not passed up to the next layer
    // One exception is height_relative, where 0 *is* relevant.
    // the highest height/time conditions (i.e. most strict)
    pub height_relative: Option<u32>,
    pub seconds_relative: u64,
    // all coins created by this spend. Duplicates are consensus failures
    pub create_coin: HashSet<NewCoin>,
    // Agg Sig Me conditions
    pub agg_sig_me: Vec<(NodePtr, NodePtr)>,
}

// these are all the conditions and properties of a complete spend bundle.
// some conditions that are created by individual spends are aggregated at the
// spend bundle level, like reserve_fee and absolute time locks. Other
// conditions are per spend, like relative time-locks and create coins (because
// they have an implied parent coin ID).
#[derive(Debug)]
pub struct SpendBundleConditions {
    pub spends: Vec<Spend>,
    // conditions
    // all these integers are initialized to 0, which also means "no
    // constraint". i.e. a 0 in these conditions are inherently satisified and
    // ignored. 0 (or negative values) are not passed up to the next layer
    // The sum of all reserve fee conditions
    pub reserve_fee: u64,
    // the highest height/time conditions (i.e. most strict)
    pub height_absolute: u32,
    pub seconds_absolute: u64,
    // Unsafe Agg Sig conditions (i.e. not tied to the spend generating it)
    pub agg_sig_unsafe: Vec<(NodePtr, NodePtr)>,

    // the total cost of the spend bundle
    pub cost: u64,
}

struct ParseState {
    // hashing of the announcements is deferred until parsing is complete. This
    // means less work up-front, in case parsing/validation fails
    announce_coin: HashSet<(Arc<[u8; 32]>, NodePtr)>,
    announce_puzzle: HashSet<(NodePtr, NodePtr)>,

    // the assert announcements are checked once everything has been parsed and
    // validated.
    assert_coin: HashSet<NodePtr>,
    assert_puzzle: HashSet<NodePtr>,

    // all coin IDs that have been spent so far. When we parse a spend we also
    // compute the coin ID, and stick it in this set. It's reference counted
    // since it may also be referenced by announcements
    spent_coins: HashSet<Arc<[u8; 32]>>,

    // this object tracks which ranges of the heap we've scanned for zeros, to
    // avoid scanning the same ranges multiple times.
    range_cache: RangeSet,
}

impl ParseState {
    fn new() -> ParseState {
        ParseState {
            announce_coin: HashSet::new(),
            announce_puzzle: HashSet::new(),
            assert_coin: HashSet::new(),
            assert_puzzle: HashSet::new(),
            spent_coins: HashSet::new(),
            range_cache: RangeSet::new(),
        }
    }
}

fn parse_spend_conditions(
    ret: &mut SpendBundleConditions,
    a: &Allocator,
    state: &mut ParseState,
    mut spend: NodePtr,
    flags: u32,
    max_cost: &mut Cost,
) -> Result<(), ValidationErr> {
    let parent_id = sanitize_hash(a, first(a, spend)?, 32, ErrorCode::InvalidParentId)?;
    spend = rest(a, spend)?;
    let puzzle_hash = sanitize_hash(a, first(a, spend)?, 32, ErrorCode::InvalidPuzzleHash)?;
    spend = rest(a, spend)?;
    let amount_buf = sanitize_uint(
        a,
        first(a, spend)?,
        8,
        ErrorCode::InvalidCoinAmount,
        &mut state.range_cache,
        flags,
    )?
    .to_vec();
    let my_amount = u64_from_bytes(&amount_buf);
    let cond = rest(a, spend)?;
    let coin_id = Arc::new(compute_coin_id(a, parent_id, puzzle_hash, &amount_buf));

    if !state.spent_coins.insert(coin_id.clone()) {
        // if this coin ID has already been added to this set, it's a double
        // spend
        return Err(ValidationErr(spend, ErrorCode::DoubleSpend));
    }

    let mut spend = Spend {
        coin_id,
        puzzle_hash,
        height_relative: None,
        seconds_relative: 0,
        create_coin: HashSet::new(),
        agg_sig_me: Vec::new(),
    };

    let mut iter = first(a, cond)?;
    while let Some((mut c, next)) = next(a, iter)? {
        iter = next;
        let op = match parse_opcode(a, first(a, c)?) {
            None => {
                // in strict mode we don't allow unknown conditions
                if (flags & STRICT_MODE) != 0 {
                    return Err(ValidationErr(c, ErrorCode::InvalidConditionOpcode));
                }
                // in non-strict mode, we just ignore unknown conditions
                continue;
            }
            Some(v) => v,
        };

        // subtract the max_cost based on the current condition
        // in case we exceed the limit, we want to fail as early as possible
        match op {
            CREATE_COIN => {
                if *max_cost < CREATE_COIN_COST {
                    return Err(ValidationErr(c, ErrorCode::CostExceeded));
                }
                *max_cost -= CREATE_COIN_COST;
            }
            AGG_SIG_UNSAFE | AGG_SIG_ME => {
                if *max_cost < AGG_SIG_COST {
                    return Err(ValidationErr(c, ErrorCode::CostExceeded));
                }
                *max_cost -= AGG_SIG_COST;
            }
            _ => (),
        }
        c = rest(a, c)?;
        let cva = parse_args(a, c, op, &mut state.range_cache, flags)?;
        match cva {
            Condition::ReserveFee(limit) => {
                // reserve fees are accumulated
                ret.reserve_fee = ret
                    .reserve_fee
                    .checked_add(limit)
                    .ok_or(ValidationErr(c, ErrorCode::ReserveFeeConditionFailed))?;
            }
            Condition::CreateCoin(ph, amount, hint) => {
                let new_coin = NewCoin {
                    puzzle_hash: a.atom(ph).to_vec(),
                    amount,
                    hint,
                };
                if !spend.create_coin.insert(new_coin) {
                    return Err(ValidationErr(c, ErrorCode::DuplicateOutput));
                }
            }
            Condition::AssertSecondsRelative(s) => {
                // keep the most strict condition. i.e. the highest limit
                spend.seconds_relative = max(spend.seconds_relative, s);
            }
            Condition::AssertSecondsAbsolute(s) => {
                // keep the most strict condition. i.e. the highest limit
                ret.seconds_absolute = max(ret.seconds_absolute, s);
            }
            Condition::AssertHeightRelative(h) => {
                // keep the most strict condition. i.e. the highest limit
                spend.height_relative = Some(max(spend.height_relative.unwrap_or(0), h));
            }
            Condition::AssertHeightAbsolute(h) => {
                // keep the most strict condition. i.e. the highest limit
                ret.height_absolute = max(ret.height_absolute, h);
            }
            Condition::AssertMyCoinId(id) => {
                if a.atom(id) != *spend.coin_id {
                    return Err(ValidationErr(c, ErrorCode::AssertMyCoinIdFailed));
                }
            }
            Condition::AssertMyAmount(amount) => {
                if amount != my_amount {
                    return Err(ValidationErr(c, ErrorCode::AssertMyAmountFailed));
                }
            }
            Condition::AssertMyParentId(id) => {
                if a.atom(id) != a.atom(parent_id) {
                    return Err(ValidationErr(c, ErrorCode::AssertMyParentIdFailed));
                }
            }
            Condition::AssertMyPuzzlehash(hash) => {
                if a.atom(hash) != a.atom(puzzle_hash) {
                    return Err(ValidationErr(c, ErrorCode::AssertMyPuzzlehashFailed));
                }
            }
            Condition::CreateCoinAnnouncement(msg) => {
                state.announce_coin.insert((spend.coin_id.clone(), msg));
            }
            Condition::CreatePuzzleAnnouncement(msg) => {
                state.announce_puzzle.insert((spend.puzzle_hash, msg));
            }
            Condition::AssertCoinAnnouncement(msg) => {
                state.assert_coin.insert(msg);
            }
            Condition::AssertPuzzleAnnouncement(msg) => {
                state.assert_puzzle.insert(msg);
            }
            Condition::AggSigMe(pk, msg) => {
                spend.agg_sig_me.push((pk, msg));
            }
            Condition::AggSigUnsafe(pk, msg) => {
                ret.agg_sig_unsafe.push((pk, msg));
            }
            Condition::Skip => {}
        }
    }

    ret.spends.push(spend);
    Ok(())
}

// This function parses, and validates aspects of, the above structure and
// returns a list of all spends, along with all conditions, organized by
// condition op-code
pub fn parse_spends(
    a: &Allocator,
    spends: NodePtr,
    max_cost: Cost,
    flags: u32,
) -> Result<SpendBundleConditions, ValidationErr> {
    let mut ret = SpendBundleConditions {
        spends: Vec::new(),
        reserve_fee: 0,
        height_absolute: 0,
        seconds_absolute: 0,
        agg_sig_unsafe: Vec::new(),
        cost: 0,
    };

    let mut state = ParseState::new();

    let mut cost_left = max_cost;

    let mut iter = first(a, spends)?;
    while let Some((spend, next)) = next(a, iter)? {
        iter = next;
        // cost_left is passed in as a mutable reference and decremented by the
        // cost of the condition (if it has a cost). This let us fail as early
        // as possible if cost is exceeded
        // this function adds the spend to the passed-in ret
        // as well as updates it with any conditions
        parse_spend_conditions(&mut ret, a, &mut state, spend, flags, &mut cost_left)?;
    }

    // check all the assert announcements
    // if there are no asserts, there is no need to hash all the announcements
    if !state.assert_coin.is_empty() {
        let mut announcements = HashSet::<[u8; 32]>::new();

        for (coin_id, announce) in state.announce_coin {
            let mut hasher = Sha256::new();
            hasher.update(&*coin_id);
            hasher.update(a.atom(announce));
            announcements.insert(hasher.finish());
        }

        for coin_assert in state.assert_coin {
            if !announcements.contains(a.atom(coin_assert)) {
                return Err(ValidationErr(
                    coin_assert,
                    ErrorCode::AssertCoinAnnouncementFailed,
                ));
            }
        }
    }

    if !state.assert_puzzle.is_empty() {
        let mut announcements = HashSet::<[u8; 32]>::new();

        for (puzzle_hash, announce) in state.announce_puzzle {
            let mut hasher = Sha256::new();
            hasher.update(a.atom(puzzle_hash));
            hasher.update(a.atom(announce));
            announcements.insert(hasher.finish());
        }

        for puzzle_assert in state.assert_puzzle {
            if !announcements.contains(a.atom(puzzle_assert)) {
                return Err(ValidationErr(
                    puzzle_assert,
                    ErrorCode::AssertPuzzleAnnouncementFailed,
                ));
            }
        }
    }

    ret.cost = max_cost - cost_left;

    Ok(ret)
}

#[cfg(test)]
fn u64_to_bytes(n: u64) -> Vec<u8> {
    let mut buf = Vec::<u8>::new();
    buf.extend_from_slice(&n.to_be_bytes());
    if (buf[0] & 0x80) != 0 {
        buf.insert(0, 0);
    } else {
        while buf.len() > 1 && buf[0] == 0 && (buf[1] & 0x80) == 0 {
            buf.remove(0);
        }
    }
    buf
}
#[cfg(test)]
use crate::node::Node;
#[cfg(test)]
use crate::number::{ptr_from_number, Number};
#[cfg(test)]
use crate::serialize::node_to_bytes;
#[cfg(test)]
use hex::FromHex;
#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
const H1: &[u8; 32] = &[
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
];
#[cfg(test)]
const H2: &[u8; 32] = &[
    2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
];

#[cfg(test)]
const LONG_VEC: &[u8; 33] = &[
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
    3,
];

#[cfg(test)]
const PUBKEY: &[u8; 48] = &[
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
    6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6, 6,
];
#[cfg(test)]
const MSG1: &[u8; 13] = &[3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3];
#[cfg(test)]
const MSG2: &[u8; 19] = &[4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4];

#[cfg(test)]
const LONGMSG: &[u8; 1025] = &[
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4, 4,
    4,
];

#[cfg(test)]
fn hash_buf(b1: &[u8], b2: &[u8]) -> Vec<u8> {
    let mut ctx = Sha256::new();
    ctx.update(b1);
    ctx.update(b2);
    ctx.finish().to_vec()
}

#[cfg(test)]
fn test_coin_id(parent_id: &[u8], puzzle_hash: &[u8], amount: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(parent_id);
    hasher.update(puzzle_hash);
    let buf = u64_to_bytes(amount);
    hasher.update(&buf);
    hasher.finish()
}

// this is a very simple parser. It does not handle errors, because it's only
// meant for tests
// * redundant white space is not supported.
// * lists are not supported, only cons boxes
// * cons boxes may not be terminated by ")". They are terminated implicitly after
//   the second atom.
// * ) means nil
// * substitutions for test values can be done with {name} in the input string.
// * arbitrary substitutions can be made with a callback and {} in the intput
//   string
// Example:
// (1 (2 (3 ) means: (1 . (2 . (3 . ())))
// and:

#[cfg(test)]
fn parse_list_impl(
    a: &mut Allocator,
    input: &str,
    callback: &Option<fn(&mut Allocator) -> NodePtr>,
    subs: &HashMap<&'static str, NodePtr>,
) -> (NodePtr, usize) {
    // skip whitespace
    if input.starts_with(" ") {
        let (n, skip) = parse_list_impl(a, &input[1..], callback, subs);
        return (n, skip + 1);
    }

    if input.starts_with(")") {
        (a.null(), 1)
    } else if input.starts_with("(") {
        let (first, step1) = parse_list_impl(a, &input[1..], callback, subs);
        let (rest, step2) = parse_list_impl(a, &input[(1 + step1)..], callback, subs);
        (a.new_pair(first, rest).unwrap(), 1 + step1 + step2)
    } else if input.starts_with("{") {
        // substitute '{X}' tokens with our test hashes and messages
        // this keeps the test cases a lot simpler
        let var = input[1..].split_once("}").unwrap().0;

        let ret = match var {
            "" => callback.unwrap()(a),
            _ => *subs.get(var).unwrap(),
        };
        (ret, var.len() + 2)
    } else if input.starts_with("0x") {
        let v = input.split_once(" ").unwrap().0;

        let buf = Vec::from_hex(v.strip_prefix("0x").unwrap()).unwrap();
        (a.new_atom(&buf).unwrap(), v.len() + 1)
    } else if input.starts_with("-") || "0123456789".contains(input.get(0..1).unwrap()) {
        let v = input.split_once(" ").unwrap().0;
        let num = Number::from_str_radix(v, 10);
        (ptr_from_number(a, &num).unwrap(), v.len() + 1)
    } else {
        panic!("atom not supported \"{}\"", input);
    }
}

#[cfg(test)]
fn parse_list(
    a: &mut Allocator,
    input: &str,
    callback: &Option<fn(&mut Allocator) -> NodePtr>,
) -> NodePtr {
    // all substitutions are allocated up-front in order to have them all use
    // the same atom in the CLVM structure. This is to cover cases where
    // conditions may be deduplicated based on the NodePtr value, when they
    // shouldn't be. The AggSig conditions are stored with NodePtr values, but
    // should never be deduplicated.
    let mut subs = HashMap::<&'static str, NodePtr>::new();

    // hashes
    subs.insert("h1", a.new_atom(H1).unwrap());
    subs.insert("h2", a.new_atom(H2).unwrap());
    subs.insert("long", a.new_atom(LONG_VEC).unwrap());
    // public key
    subs.insert("pubkey", a.new_atom(PUBKEY).unwrap());
    // announce/aggsig messages
    subs.insert("msg1", a.new_atom(MSG1).unwrap());
    subs.insert("msg2", a.new_atom(MSG2).unwrap());
    subs.insert("longmsg", a.new_atom(LONGMSG).unwrap());
    // coin IDs
    subs.insert("coin11", a.new_atom(&test_coin_id(H1, H1, 123)).unwrap());
    subs.insert("coin12", a.new_atom(&test_coin_id(H1, H2, 123)).unwrap());
    subs.insert("coin21", a.new_atom(&test_coin_id(H2, H1, 123)).unwrap());
    subs.insert("coin22", a.new_atom(&test_coin_id(H2, H2, 123)).unwrap());
    // coin announcements
    subs.insert(
        "c11",
        a.new_atom(&hash_buf(&test_coin_id(H1, H2, 123), MSG1))
            .unwrap(),
    );
    subs.insert(
        "c21",
        a.new_atom(&hash_buf(&test_coin_id(H2, H2, 123), MSG1))
            .unwrap(),
    );
    subs.insert(
        "c12",
        a.new_atom(&hash_buf(&test_coin_id(H1, H2, 123), MSG2))
            .unwrap(),
    );
    subs.insert(
        "c22",
        a.new_atom(&hash_buf(&test_coin_id(H2, H2, 123), MSG2))
            .unwrap(),
    );
    // puzzle announcements
    subs.insert("p11", a.new_atom(&hash_buf(H1, MSG1)).unwrap());
    subs.insert("p21", a.new_atom(&hash_buf(H2, MSG1)).unwrap());
    subs.insert("p12", a.new_atom(&hash_buf(H1, MSG2)).unwrap());
    subs.insert("p22", a.new_atom(&hash_buf(H2, MSG2)).unwrap());

    let (n, count) = parse_list_impl(a, input, callback, &subs);
    assert_eq!(&input[count..], "");
    n
}

// The callback can be used for arbitrary substitutions using {} in the input
// string. Since the parser is recursive and simple, large structures have to be
// constructed this way
#[cfg(test)]
fn cond_test_cb(
    input: &str,
    callback: Option<fn(&mut Allocator) -> NodePtr>,
) -> Result<(Allocator, SpendBundleConditions), ValidationErr> {
    let mut a = Allocator::new();

    println!("input: {}", input);

    let n = parse_list(&mut a, &input, &callback);
    for c in node_to_bytes(&Node::new(&a, n)).unwrap() {
        print!("{:02x}", c);
    }
    println!();
    let flags: u32 = 0;
    match parse_spends(&a, n, 11000000000, flags) {
        Ok(list) => {
            for n in &list.spends {
                println!("{:?}", n);
            }
            Ok((a, list))
        }
        Err(e) => Err(e),
    }
}

#[cfg(test)]
fn cond_test(input: &str) -> Result<(Allocator, SpendBundleConditions), ValidationErr> {
    cond_test_cb(input, None)
}

#[test]
fn test_invalid_condition_list1() {
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (8 )))").unwrap_err().1,
        ErrorCode::InvalidCondition
    );
}

#[test]
fn test_invalid_condition_list2() {
    assert_eq!(
        cond_test("((({h1} ({h2} (123 ((8 ))))").unwrap_err().1,
        ErrorCode::InvalidCondition
    );
}

#[test]
fn test_invalid_condition_args_terminator() {
    // we only look at the condition arguments the condition expects, any
    // additional arguments are ignored, including the terminator
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((80 (50 8 ))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    assert_eq!(spend.seconds_relative, 50);
}
#[test]
fn test_invalid_condition_list_terminator() {
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((80 (50 8 ))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    assert_eq!(spend.seconds_relative, 50);
}

#[test]
fn test_invalid_condition_short_list_terminator() {
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((80 8 ))))").unwrap_err().1,
        ErrorCode::InvalidCondition
    );
}

#[test]
fn test_invalid_spend_list1() {
    assert_eq!(
        cond_test("(8 )").unwrap_err().1,
        ErrorCode::InvalidCondition
    );
}

#[test]
fn test_invalid_spend_list2() {
    assert_eq!(
        cond_test("((8 ))").unwrap_err().1,
        ErrorCode::InvalidCondition
    );
}

#[test]
fn test_invalid_spend_list_terminator() {
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (()) 8 ))").unwrap_err().1,
        ErrorCode::InvalidCondition
    );
}

#[test]
fn test_single_seconds_relative() {
    // ASSERT_SECONDS_RELATIVE
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((80 (101 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    assert_eq!(spend.seconds_relative, 101);
}

#[test]
fn test_seconds_relative_exceed_max() {
    // ASSERT_SECONDS_RELATIVE
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((80 (0x010000000000000000 )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertSecondsRelative
    );
}

#[test]
fn test_multiple_seconds_relative() {
    // ASSERT_SECONDS_RELATIVE
    let (a, conds) =
        cond_test("((({h1} ({h2} (123 (((80 (100 ) ((80 (503 ) ((80 (90 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    // we use the MAX value
    assert_eq!(spend.seconds_relative, 503);
}

#[test]
fn test_single_seconds_absolute() {
    // ASSERT_SECONDS_ABSOLUTE
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((81 (104 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    assert_eq!(conds.seconds_absolute, 104);
}

#[test]
fn test_seconds_absolute_exceed_max() {
    // ASSERT_SECONDS_ABSOLUTE
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((81 (0x010000000000000000 )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertSecondsAbsolute
    );
}

#[test]
fn test_multiple_seconds_absolute() {
    // ASSERT_SECONDS_ABSOLUTE
    let (a, conds) =
        cond_test("((({h1} ({h2} (123 (((81 (100 ) ((81 (503 ) ((81 (90 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    // we use the MAX value
    assert_eq!(conds.seconds_absolute, 503);
}

#[test]
fn test_single_height_relative() {
    // ASSERT_HEIGHT_RELATIVE
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((82 (101 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    assert_eq!(spend.height_relative, Some(101));
}

#[test]
fn test_single_height_relative_zero() {
    // ASSERT_HEIGHT_RELATIVE
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((82 (0 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    assert_eq!(spend.height_relative, Some(0));
}

#[test]
fn test_height_relative_exceed_max() {
    // ASSERT_HEIGHT_RELATIVE
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((82 (0x0100000000 )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertHeightRelative
    );
}

#[test]
fn test_multiple_height_relative() {
    // ASSERT_HEIGHT_RELATIVE
    let (a, conds) =
        cond_test("((({h1} ({h2} (123 (((82 (100 ) ((82 (503 ) ((82 (90 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    // we use the MAX value
    assert_eq!(spend.height_relative, Some(503));
}

#[test]
fn test_single_height_absolute() {
    // ASSERT_HEIGHT_ABSOLUTE
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((83 (100 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    assert_eq!(conds.height_absolute, 100);
}

#[test]
fn test_height_absolute_exceed_max() {
    // ASSERT_HEIGHT_ABSOLUTE
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((83 (0x0100000000 )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertHeightAbsolute
    );
}

#[test]
fn test_multiple_height_absolute() {
    // ASSERT_HEIGHT_ABSOLUTE
    let (a, conds) =
        cond_test("((({h1} ({h2} (123 (((83 (100 ) ((83 (503 ) ((83 (90 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    // we use the MAX value
    assert_eq!(conds.height_absolute, 503);
}

#[test]
fn test_single_reserve_fee() {
    // RESERVE_FEE
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((52 (100 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    assert_eq!(conds.reserve_fee, 100);
}

#[test]
fn test_reserve_fee_exceed_max() {
    // RESERVE_FEE)
    // 0xfffffffffffffff0 + 0x10 just exceeds u64::MAX, which is higher than
    // allowed
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((52 (0x00fffffffffffffff0 ) ((52 (0x10 ) ))))")
            .unwrap_err()
            .1,
        ErrorCode::ReserveFeeConditionFailed
    );
}

#[test]
fn test_multiple_reserve_fee() {
    // RESERVE_FEE
    let (a, conds) =
        cond_test("((({h1} ({h2} (123 (((52 (100 ) ((52 (25 ) ((52 (50 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);

    // reserve fee conditions are accumulated 100 + 50 = 150
    assert_eq!(conds.reserve_fee, 175);
}

// TOOD: test announcement across coins

#[test]
fn test_coin_announces_consume() {
    // CREATE_COIN_ANNOUNCEMENT
    // ASSERT_COIN_ANNOUNCEMENT
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((60 ({msg1} ) ((61 ({c11} )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
}

#[test]
fn test_cross_coin_announces_consume() {
    // CREATE_COIN_ANNOUNCEMENT
    // ASSERT_COIN_ANNOUNCEMENT
    let (a, conds) =
        cond_test("((({h1} ({h2} (123 (((60 ({msg1} ))) (({h2} ({h2} (123 (((61 ({c11} )))))")
            .unwrap();

    assert_eq!(conds.spends.len(), 2);
    assert_eq!(*conds.spends[0].coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(conds.spends[0].puzzle_hash), H2);
    assert_eq!(*conds.spends[1].coin_id, test_coin_id(H2, H2, 123));
    assert_eq!(a.atom(conds.spends[1].puzzle_hash), H2);
}

#[test]
fn test_coin_announce_missing_arg() {
    // CREATE_COIN_ANNOUNCEMENT
    // ASSERT_COIN_ANNOUNCEMENT
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((60 ) ((61 ({p21} )))))")
            .unwrap_err()
            .1,
        ErrorCode::InvalidCondition
    );
}

#[test]
fn test_failing_coin_consume() {
    // ASSERT_COIN_ANNOUNCEMENT
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((61 ({c11} )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertCoinAnnouncementFailed
    );
}

#[test]
fn test_coin_announce_mismatch() {
    // CREATE_COIN_ANNOUNCEMENT
    // ASSERT_COIN_ANNOUNCEMENT
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((60 ({msg1} ) ((61 ({c12} )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertCoinAnnouncementFailed
    );
}

#[test]
fn test_puzzle_announces_consume() {
    // CREATE_PUZZLE_ANNOUNCEMENT
    // ASSERT_PUZZLE_ANNOUNCEMENT
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((62 ({msg1} ) ((63 ({p21} )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
}

#[test]
fn test_cross_coin_puzzle_announces_consume() {
    // CREATE_PUZZLE_ANNOUNCEMENT
    // ASSERT_PUZZLE_ANNOUNCEMENT
    let (a, conds) =
        cond_test("((({h1} ({h2} (123 (((62 ({msg1} ))) (({h2} ({h2} (123 (((63 ({p21} )))))")
            .unwrap();

    assert_eq!(conds.spends.len(), 2);
    assert_eq!(*conds.spends[0].coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(conds.spends[0].puzzle_hash), H2);
    assert_eq!(*conds.spends[1].coin_id, test_coin_id(H2, H2, 123));
    assert_eq!(a.atom(conds.spends[1].puzzle_hash), H2);
}

#[test]
fn test_puzzle_announce_missing_arg() {
    // CREATE_PUZZLE_ANNOUNCEMENT
    // ASSERT_PUZZLE_ANNOUNCEMENT
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((62 ) ((63 ({p21} )))))")
            .unwrap_err()
            .1,
        ErrorCode::InvalidCondition
    );
}

#[test]
fn test_failing_puzzle_consume() {
    // ASSERT_PUZZLE_ANNOUNCEMENT
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((63 ({p21} )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertPuzzleAnnouncementFailed
    );
}

#[test]
fn test_puzzle_announce_mismatch() {
    // CREATE_PUZZLE_ANNOUNCEMENT
    // ASSERT_PUZZLE_ANNOUNCEMENT
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((62 ({msg1} ) ((63 ({p11} )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertPuzzleAnnouncementFailed
    );
}

#[test]
fn test_single_assert_my_amount() {
    // ASSERT_MY_AMOUNT
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((73 (123 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
}

#[test]
fn test_single_assert_my_amount_exceed_max() {
    // ASSERT_MY_AMOUNT
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((73 (0x010000000000000000 )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertMyAmountFailed
    );
}

#[test]
fn test_single_assert_my_amount_overlong() {
    // ASSERT_MY_AMOUNT
    // leading zeroes are ignored
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((73 (0x0000007b )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
}

#[test]
fn test_multiple_assert_my_amount() {
    // ASSERT_MY_AMOUNT
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((73 (123 ) ((73 (123 ) ))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
}

#[test]
fn test_multiple_failing_assert_my_amount() {
    // ASSERT_MY_AMOUNT
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((73 (123 ) ((73 (122 ) ))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertMyAmountFailed
    );
}

#[test]
fn test_single_failing_assert_my_amount() {
    // ASSERT_MY_AMOUNT
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((73 (124 ) ))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertMyAmountFailed
    );
}

#[test]
fn test_single_assert_my_coin_id() {
    // ASSERT_MY_COIN_ID
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((70 ({coin12} )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
}

#[test]
fn test_single_assert_my_coin_id_overlong() {
    // ASSERT_MY_COIN_ID
    // leading zeros in the coin amount are ignored when computing the coin ID
    let (a, conds) = cond_test("((({h1} ({h2} (0x0000007b (((70 ({coin12} )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
}

#[test]
fn test_multiple_assert_my_coin_id() {
    // ASSERT_MY_COIN_ID
    let (a, conds) =
        cond_test("((({h1} ({h2} (123 (((70 ({coin12} ) ((70 ({coin12} ) ))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
}

#[test]
fn test_single_assert_my_coin_id_mismatch() {
    // ASSERT_MY_COIN_ID
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((70 ({coin11} )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertMyCoinIdFailed
    );
}

#[test]
fn test_multiple_assert_my_coin_id_mismatch() {
    // ASSERT_MY_COIN_ID
    // ASSERT_MY_AMOUNT
    // the coin-ID check matches the *other* coin, not itself
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((60 (123 ))) (({h1} ({h1} (123 (((70 ({coin12} )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertMyCoinIdFailed
    );
}

#[test]
fn test_single_assert_my_parent_coin_id() {
    // ASSERT_MY_PARENT_ID
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((71 ({h1} )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
}

#[test]
fn test_multiple_assert_my_parent_coin_id() {
    // ASSERT_MY_PARENT_ID
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((71 ({h1} ) ((71 ({h1} ) ))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
}

#[test]
fn test_single_assert_my_parent_coin_id_mismatch() {
    // ASSERT_MY_PARENT_ID
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((71 ({h2} )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertMyParentIdFailed
    );
}

#[test]
fn test_single_invalid_assert_my_parent_coin_id() {
    // ASSERT_MY_PARENT_ID
    // the parent ID in the condition is 33 bytes long
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((71 ({long} )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertMyParentIdFailed
    );
}

#[test]
fn test_single_assert_my_puzzle_hash() {
    // ASSERT_MY_PUZZLEHASH
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((72 ({h2} )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
}

#[test]
fn test_multiple_assert_my_puzzle_hash() {
    // ASSERT_MY_PUZZLEHASH
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((72 ({h2} ) ((72 ({h2} ) ))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
}

#[test]
fn test_single_assert_my_puzzle_hash_mismatch() {
    // ASSERT_MY_PUZZLEHASH
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((72 ({h1} )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertMyPuzzlehashFailed
    );
}

#[test]
fn test_single_invalid_assert_my_puzzle_hash() {
    // ASSERT_MY_PUZZLEHASH
    // the parent ID in the condition is 33 bytes long
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((72 ({long} )))))")
            .unwrap_err()
            .1,
        ErrorCode::AssertMyPuzzlehashFailed
    );
}

#[test]
fn test_single_create_coin() {
    // CREATE_COIN
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((51 ({h2} (42 )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.create_coin.len(), 1);
    for c in &spend.create_coin {
        assert_eq!(c.puzzle_hash, H2.to_vec());
        assert_eq!(c.amount, 42_u64);
        assert_eq!(c.hint, a.null());
    }
}

#[test]
fn test_create_coin_max_amount() {
    // CREATE_COIN
    let (a, conds) =
        cond_test("((({h1} ({h2} (123 (((51 ({h2} (0x00ffffffffffffffff )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.create_coin.len(), 1);
    for c in &spend.create_coin {
        assert_eq!(c.puzzle_hash, H2.to_vec());
        assert_eq!(c.amount, 0xffffffffffffffff_u64);
        assert_eq!(c.hint, a.null());
    }
}

#[test]
fn test_create_coin_amount_exceeds_max() {
    // CREATE_COIN
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((51 ({h2} (0x010000000000000000 )))))")
            .unwrap_err()
            .1,
        ErrorCode::InvalidCoinAmount
    );
}

#[test]
fn test_create_coin_negative_amount() {
    // CREATE_COIN
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((51 ({h2} (-1 )))))")
            .unwrap_err()
            .1,
        ErrorCode::InvalidCoinAmount
    );
}

#[test]
fn test_create_coin_invalid_puzzlehash() {
    // CREATE_COIN
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((51 ({long} (42 )))))")
            .unwrap_err()
            .1,
        ErrorCode::InvalidPuzzleHash
    );
}

#[test]
fn test_create_coin_with_hint() {
    // CREATE_COIN
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((51 ({h2} (42 (({h1}) )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.create_coin.len(), 1);
    for c in &spend.create_coin {
        assert!(c.puzzle_hash == H2.to_vec());
        assert!(c.amount == 42_u64);
        assert!(a.atom(c.hint) == H1.to_vec());
    }
}

#[test]
fn test_create_coin_with_multiple_hints() {
    // CREATE_COIN
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((51 ({h2} (42 (({h1} ({h2}) )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.create_coin.len(), 1);
    for c in &spend.create_coin {
        assert!(c.puzzle_hash == H2.to_vec());
        assert!(c.amount == 42_u64);
        assert!(a.atom(c.hint) == H1.to_vec());
    }
}

#[test]
fn test_create_coin_with_hint_as_atom() {
    // CREATE_COIN
    // the hint is supposed to be a list
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((51 ({h2} (42 ({h1} )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.create_coin.len(), 1);
    for c in &spend.create_coin {
        assert_eq!(c.puzzle_hash, H2.to_vec());
        assert_eq!(c.amount, 42_u64);
        assert_eq!(c.hint, a.null());
    }
}

#[test]
fn test_create_coin_with_invalid_hint_as_terminator() {
    // CREATE_COIN
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((51 ({h2} (42 {h1}))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.create_coin.len(), 1);
    for c in &spend.create_coin {
        assert_eq!(c.puzzle_hash, H2.to_vec());
        assert_eq!(c.amount, 42_u64);
        assert_eq!(c.hint, a.null());
    }
}

#[test]
fn test_create_coin_with_short_hint() {
    // CREATE_COIN
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((51 ({h2} (42 (({msg1}))))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.create_coin.len(), 1);

    for c in &spend.create_coin {
        assert!(c.puzzle_hash == H2.to_vec());
        assert!(c.amount == 42_u64);
        assert!(a.atom(c.hint) == MSG1.to_vec());
    }
}

#[test]
fn test_create_coin_with_long_hint() {
    // CREATE_COIN
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((51 ({h2} (42 ({long})))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.create_coin.len(), 1);

    for c in &spend.create_coin {
        assert_eq!(c.puzzle_hash, H2.to_vec());
        assert_eq!(c.amount, 42_u64);
        assert_eq!(c.hint, a.null());
    }
}

#[test]
fn test_create_coin_with_pair_hint() {
    // CREATE_COIN
    // we only pick out the first element
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((51 ({h2} (42 (({h1} {h2} )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.create_coin.len(), 1);

    for c in &spend.create_coin {
        assert_eq!(c.puzzle_hash, H2.to_vec());
        assert_eq!(c.amount, 42_u64);
        assert_eq!(a.atom(c.hint), H1.to_vec());
    }
}

#[test]
fn test_create_coin_with_cons_hint() {
    // CREATE_COIN
    // if the first element is a cons-box, it's not a valid hint
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((51 ({h2} (42 ((({h1} {h2}) )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.create_coin.len(), 1);
    for c in &spend.create_coin {
        assert_eq!(c.puzzle_hash, H2.to_vec());
        assert_eq!(c.amount, 42_u64);
        assert_eq!(c.hint, a.null());
    }
}

#[test]
fn test_multiple_create_coin() {
    // CREATE_COIN
    let (a, conds) =
        cond_test("((({h1} ({h2} (123 (((51 ({h2} (42 ) ((51 ({h2} (43 ) ))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.create_coin.len(), 2);

    assert!(spend.create_coin.contains(&NewCoin {
        puzzle_hash: H2.to_vec(),
        amount: 42_u64,
        hint: a.null()
    }));
    assert!(spend.create_coin.contains(&NewCoin {
        puzzle_hash: H2.to_vec(),
        amount: 43_u64,
        hint: a.null()
    }));
}

#[test]
fn test_create_coin_exceed_cost() {
    // CREATE_COIN
    // ensure that we terminate parsing conditions once they exceed the max cost
    assert_eq!(
        cond_test_cb(
            "((({h1} ({h2} (123 ({} )))",
            Some(|a: &mut Allocator| -> NodePtr {
                let mut rest: NodePtr = a.null();

                for i in 0..6500 {
                    // this builds one CREATE_COIN condition
                    // borrow-rules prevent this from being succint
                    let coin = a.null();
                    let val = a.new_atom(&u64_to_bytes(i)).unwrap();
                    let coin = a.new_pair(val, coin).unwrap();
                    let val = a.new_atom(H2).unwrap();
                    let coin = a.new_pair(val, coin).unwrap();
                    let val = a.new_atom(&u64_to_bytes(CREATE_COIN as u64)).unwrap();
                    let coin = a.new_pair(val, coin).unwrap();

                    // add the CREATE_COIN condition to the list (called rest)
                    rest = a.new_pair(coin, rest).unwrap();
                }
                rest
            })
        )
        .unwrap_err()
        .1,
        ErrorCode::CostExceeded
    );
}

#[test]
fn test_duplicate_create_coin() {
    // CREATE_COIN
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((51 ({h2} (42 ) ((51 ({h2} (42 ) ))))")
            .unwrap_err()
            .1,
        ErrorCode::DuplicateOutput
    );
}

#[test]
fn test_single_agg_sig_me() {
    // AGG_SIG_ME
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((50 ({pubkey} ({msg1} )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.agg_sig_me.len(), 1);
    for c in &spend.agg_sig_me {
        assert_eq!(a.atom(c.0), PUBKEY);
        assert_eq!(a.atom(c.1), MSG1);
    }
}

#[test]
fn test_duplicate_agg_sig_me() {
    // AGG_SIG_ME
    // we cannot deduplicate AGG_SIG conditions. Their signatures will be
    // aggregated, and so must all copies of the public keys
    let (a, conds) =
        cond_test("((({h1} ({h2} (123 (((50 ({pubkey} ({msg1} ) ((50 ({pubkey} ({msg1} ) ))))")
            .unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(spend.agg_sig_me.len(), 2);
    for c in &spend.agg_sig_me {
        assert_eq!(a.atom(c.0), PUBKEY);
        assert_eq!(a.atom(c.1), MSG1);
    }
}

#[test]
fn test_agg_sig_me_invalid_pubkey() {
    // AGG_SIG_ME
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((50 ({h2} ({msg1} )))))")
            .unwrap_err()
            .1,
        ErrorCode::InvalidPubkey
    );
}

#[test]
fn test_agg_sig_me_invalid_msg() {
    // AGG_SIG_ME
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((50 ({pubkey} ({longmsg} )))))")
            .unwrap_err()
            .1,
        ErrorCode::InvalidMessage
    );
}

#[test]
fn test_agg_sig_me_exceed_cost() {
    // AGG_SIG_ME
    // ensure that we terminate parsing conditions once they exceed the max cost
    assert_eq!(
        cond_test_cb(
            "((({h1} ({h2} (123 ({} )))",
            Some(|a: &mut Allocator| -> NodePtr {
                let mut rest: NodePtr = a.null();

                for _i in 0..9167 {
                    // this builds one AGG_SIG_ME condition
                    // borrow-rules prevent this from being succint
                    let aggsig = a.null();
                    let val = a.new_atom(MSG1).unwrap();
                    let aggsig = a.new_pair(val, aggsig).unwrap();
                    let val = a.new_atom(PUBKEY).unwrap();
                    let aggsig = a.new_pair(val, aggsig).unwrap();
                    let val = a.new_atom(&u64_to_bytes(AGG_SIG_ME as u64)).unwrap();
                    let aggsig = a.new_pair(val, aggsig).unwrap();

                    // add the AGG_SIG_ME condition to the list (called rest)
                    rest = a.new_pair(aggsig, rest).unwrap();
                }
                rest
            })
        )
        .unwrap_err()
        .1,
        ErrorCode::CostExceeded
    );
}

#[test]
fn test_single_agg_sig_unsafe() {
    // AGG_SIG_UNSAFE
    let (a, conds) = cond_test("((({h1} ({h2} (123 (((49 ({pubkey} ({msg1} )))))").unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(conds.agg_sig_unsafe.len(), 1);
    for (pk, msg) in &conds.agg_sig_unsafe {
        assert_eq!(a.atom(*pk), PUBKEY);
        assert_eq!(a.atom(*msg), MSG1);
    }
}

#[test]
fn test_agg_sig_unsafe_extra_arg() {
    // AGG_SIG_UNSAFE
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((49 ({pubkey} ({msg1} (456 )))))")
            .unwrap_err()
            .1,
        ErrorCode::InvalidCondition
    );
}

#[test]
fn test_agg_sig_me_extra_arg() {
    // AGG_SIG_ME
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((50 ({pubkey} ({msg1} (456 )))))")
            .unwrap_err()
            .1,
        ErrorCode::InvalidCondition
    );
}

#[test]
fn test_duplicate_agg_sig_unsafe() {
    // AGG_SIG_UNSAFE
    // these conditions may not be deduplicated
    let (a, conds) =
        cond_test("((({h1} ({h2} (123 (((49 ({pubkey} ({msg1} ) ((49 ({pubkey} ({msg1} ) ))))")
            .unwrap();

    assert_eq!(conds.spends.len(), 1);
    let spend = &conds.spends[0];
    assert_eq!(*spend.coin_id, test_coin_id(H1, H2, 123));
    assert_eq!(a.atom(spend.puzzle_hash), H2);
    assert_eq!(conds.agg_sig_unsafe.len(), 2);
    for (pk, msg) in &conds.agg_sig_unsafe {
        assert_eq!(a.atom(*pk), PUBKEY);
        assert_eq!(a.atom(*msg), MSG1);
    }
}

#[test]
fn test_agg_sig_unsafe_invalid_pubkey() {
    // AGG_SIG_UNSAFE
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((49 ({h2} ({msg1} )))))")
            .unwrap_err()
            .1,
        ErrorCode::InvalidPubkey
    );
}

#[test]
fn test_agg_sig_unsafe_invalid_msg() {
    // AGG_SIG_UNSAFE
    assert_eq!(
        cond_test("((({h1} ({h2} (123 (((49 ({pubkey} ({longmsg} )))))")
            .unwrap_err()
            .1,
        ErrorCode::InvalidMessage
    );
}

#[test]
fn test_agg_sig_unsafe_exceed_cost() {
    // AGG_SIG_UNSAFE
    // ensure that we terminate parsing conditions once they exceed the max cost
    assert_eq!(
        cond_test_cb(
            "((({h1} ({h2} (123 ({} )))",
            Some(|a: &mut Allocator| -> NodePtr {
                let mut rest: NodePtr = a.null();

                for _i in 0..9167 {
                    // this builds one AGG_SIG_UNSAFE condition
                    // borrow-rules prevent this from being succint
                    let aggsig = a.null();
                    let val = a.new_atom(MSG1).unwrap();
                    let aggsig = a.new_pair(val, aggsig).unwrap();
                    let val = a.new_atom(PUBKEY).unwrap();
                    let aggsig = a.new_pair(val, aggsig).unwrap();
                    let val = a.new_atom(&u64_to_bytes(AGG_SIG_UNSAFE as u64)).unwrap();
                    let aggsig = a.new_pair(val, aggsig).unwrap();

                    // add the AGG_SIG_UNSAFE condition to the list (called rest)
                    rest = a.new_pair(aggsig, rest).unwrap();
                }
                rest
            })
        )
        .unwrap_err()
        .1,
        ErrorCode::CostExceeded
    );
}

#[test]
fn test_spend_amount_exceeds_max() {
    // the coin we're trying to spend has an amount that exceeds maximum
    assert_eq!(
        cond_test("((({h1} ({h2} (0x010000000000000000 ())))")
            .unwrap_err()
            .1,
        ErrorCode::InvalidCoinAmount
    );
}

#[test]
fn test_single_spend_negative_amount() {
    // the coin we're trying to spend has a negative amount (i.e. it's invalid)
    assert_eq!(
        cond_test("((({h1} ({h2} (-123 ())))").unwrap_err().1,
        ErrorCode::NegativeAmount
    );
}

#[test]
fn test_single_spend_invalid_puzle_hash() {
    // the puzzle hash in the spend is 33 bytes
    assert_eq!(
        cond_test("((({h1} ({long} (123 ())))").unwrap_err().1,
        ErrorCode::InvalidPuzzleHash
    );
}

#[test]
fn test_single_spend_invalid_parent_id() {
    // the parent coin ID is 33 bytes long
    assert_eq!(
        cond_test("((({long} ({h2} (123 ())))").unwrap_err().1,
        ErrorCode::InvalidParentId
    );
}

#[test]
fn test_double_spend() {
    // we spend the same coin twice
    assert_eq!(
        cond_test("((({h1} ({h2} (123 ()) (({h1} ({h2} (123 ())))")
            .unwrap_err()
            .1,
        ErrorCode::DoubleSpend
    );
}
