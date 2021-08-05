use super::rangeset::RangeSet;
use super::sanitize_int::sanitize_uint;
use super::validation_error::{atom, ErrorCode, ValidationErr};
use clvm_rs::allocator::{Allocator, NodePtr};

pub fn sanitize_hash(
    a: &Allocator,
    n: NodePtr,
    size: usize,
    code: ErrorCode,
) -> Result<NodePtr, ValidationErr> {
    let buf = atom(a, n, code)?;

    if buf.len() != size {
        Err(ValidationErr(n, code))
    } else {
        Ok(n)
    }
}

pub fn parse_amount(
    a: &Allocator,
    n: NodePtr,
    code: ErrorCode,
    range_cache: &mut RangeSet,
    flags: u32,
) -> Result<u64, ValidationErr> {
    // amounts are not allowed to exceed 2^64. i.e. 8 bytes
    match sanitize_uint(a, n, 8, code, range_cache, flags) {
        Err(ValidationErr(n, ErrorCode::NegativeAmount)) => Err(ValidationErr(n, code)),
        Err(r) => Err(r),
        Ok(r) => Ok(u64_from_bytes(r)),
    }
}

// a negative height is always true. In this case the
// condition can be ignored and this functon returns 0
pub fn parse_height(
    a: &Allocator,
    n: NodePtr,
    code: ErrorCode,
    range_cache: &mut RangeSet,
    flags: u32,
) -> Result<u32, ValidationErr> {
    // heights are not allowed to exceed 2^32. i.e. 4 bytes
    match sanitize_uint(a, n, 4, code, range_cache, flags) {
        // Height is always positive, so a negative requirement is always true,
        // just like 0.
        Err(ValidationErr(_, ErrorCode::NegativeAmount)) => Ok(0),
        Err(r) => Err(r),
        Ok(r) => Ok(u64_from_bytes(r) as u32),
    }
}

// negative seconds are always valid conditions, and will return 0
pub fn parse_seconds(
    a: &Allocator,
    n: NodePtr,
    code: ErrorCode,
    range_cache: &mut RangeSet,
    flags: u32,
) -> Result<u64, ValidationErr> {
    // seconds are not allowed to exceed 2^64. i.e. 8 bytes
    match sanitize_uint(a, n, 8, code, range_cache, flags) {
        // seconds is always positive, so a negative requirement is always true,
        // we don't need to include this condition
        Err(ValidationErr(_, ErrorCode::NegativeAmount)) => Ok(0),
        Err(r) => Err(r),
        Ok(r) => Ok(u64_from_bytes(r)),
    }
}

pub fn sanitize_announce_msg(
    a: &Allocator,
    n: NodePtr,
    code: ErrorCode,
) -> Result<NodePtr, ValidationErr> {
    let buf = atom(a, n, code)?;

    if buf.len() > 1024 {
        Err(ValidationErr(n, code))
    } else {
        Ok(n)
    }
}

#[cfg(test)]
use clvm_rs::run_program::STRICT_MODE;

#[cfg(test)]
fn zero_vec(len: usize) -> Vec<u8> {
    let mut ret = Vec::<u8>::new();
    for _i in 0..len {
        ret.push(0);
    }
    ret
}

#[test]
fn test_sanitize_hash() {
    let mut a = Allocator::new();
    let short = zero_vec(31);
    let valid = zero_vec(32);
    let long = zero_vec(33);

    let short_n = a.new_atom(&short).unwrap();
    assert_eq!(
        sanitize_hash(&a, short_n, 32, ErrorCode::InvalidCondition),
        Err(ValidationErr(short_n, ErrorCode::InvalidCondition))
    );
    let valid_n = a.new_atom(&valid).unwrap();
    assert_eq!(
        sanitize_hash(&a, valid_n, 32, ErrorCode::InvalidCondition),
        Ok(valid_n)
    );
    let long_n = a.new_atom(&long).unwrap();
    assert_eq!(
        sanitize_hash(&a, long_n, 32, ErrorCode::InvalidCondition),
        Err(ValidationErr(long_n, ErrorCode::InvalidCondition))
    );

    let pair = a.new_pair(short_n, long_n).unwrap();
    assert_eq!(
        sanitize_hash(&a, pair, 32, ErrorCode::InvalidCondition),
        Err(ValidationErr(pair, ErrorCode::InvalidCondition))
    );
}

#[test]
fn test_sanitize_announce_msg() {
    let mut a = Allocator::new();
    let valid = zero_vec(1024);
    let valid_n = a.new_atom(&valid).unwrap();
    assert_eq!(
        sanitize_announce_msg(&a, valid_n, ErrorCode::InvalidCondition),
        Ok(valid_n)
    );

    let long = zero_vec(1025);
    let long_n = a.new_atom(&long).unwrap();
    assert_eq!(
        sanitize_announce_msg(&a, long_n, ErrorCode::InvalidCondition),
        Err(ValidationErr(long_n, ErrorCode::InvalidCondition))
    );

    let pair = a.new_pair(valid_n, long_n).unwrap();
    assert_eq!(
        sanitize_announce_msg(&a, pair, ErrorCode::InvalidCondition),
        Err(ValidationErr(pair, ErrorCode::InvalidCondition))
    );
}

#[cfg(test)]
fn amount_tester(buf: &[u8], flags: u32) -> Result<u64, ValidationErr> {
    let mut a = Allocator::new();
    let n = a.new_atom(buf).unwrap();
    let mut range_cache = RangeSet::new();

    parse_amount(
        &mut a,
        n,
        ErrorCode::InvalidCoinAmount,
        &mut range_cache,
        flags,
    )
}

#[test]
fn test_sanitize_amount() {
    for flags in &[0, STRICT_MODE] {
        // negative amounts are not allowed
        assert_eq!(
            amount_tester(&[0x80], *flags).unwrap_err().1,
            ErrorCode::InvalidCoinAmount
        );
        assert_eq!(
            amount_tester(&[0xff], *flags).unwrap_err().1,
            ErrorCode::InvalidCoinAmount
        );
        assert_eq!(
            amount_tester(&[0xff, 0], *flags).unwrap_err().1,
            ErrorCode::InvalidCoinAmount
        );

        // leading zeros are somtimes necessary to make values positive
        assert_eq!(amount_tester(&[0, 0xff], *flags), Ok(0xff));
        // but are stripped when they are redundant
        if (flags & STRICT_MODE) != 0 {
            assert_eq!(
                amount_tester(&[0, 0, 0, 0xff], *flags).unwrap_err().1,
                ErrorCode::InvalidCoinAmount
            );
            assert_eq!(
                amount_tester(&[0, 0, 0, 0x80], *flags).unwrap_err().1,
                ErrorCode::InvalidCoinAmount
            );
            assert_eq!(
                amount_tester(&[0, 0, 0, 0x7f], *flags).unwrap_err().1,
                ErrorCode::InvalidCoinAmount
            );
            assert_eq!(
                amount_tester(&[0, 0, 0], *flags).unwrap_err().1,
                ErrorCode::InvalidCoinAmount
            );
        } else {
            assert_eq!(amount_tester(&[0, 0, 0, 0xff], *flags), Ok(0xff));
            assert_eq!(amount_tester(&[0, 0, 0, 0x80], *flags), Ok(0x80));
            assert_eq!(amount_tester(&[0, 0, 0, 0x7f], *flags), Ok(0x7f));
            assert_eq!(amount_tester(&[0, 0, 0], *flags), Ok(0));
        }

        // amounts aren't allowed to be too big
        assert_eq!(
            amount_tester(&[0x7f, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0], *flags)
                .unwrap_err()
                .1,
            ErrorCode::InvalidCoinAmount
        );

        // this is small enough though
        assert_eq!(
            amount_tester(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff], *flags),
            Ok(0xffffffffffffffff)
        );
    }
}

#[cfg(test)]
fn height_tester(buf: &[u8], flags: u32) -> Result<u32, ValidationErr> {
    let mut a = Allocator::new();
    let n = a.new_atom(buf).unwrap();
    let mut range_cache = RangeSet::new();

    parse_height(
        &mut a,
        n,
        ErrorCode::AssertHeightAbsolute,
        &mut range_cache,
        flags,
    )
}

#[test]
fn test_parse_height() {
    for flags in &[0, STRICT_MODE] {
        // negative heights can be ignored
        assert_eq!(height_tester(&[0x80], *flags), Ok(0));
        assert_eq!(height_tester(&[0xff], *flags), Ok(0));
        assert_eq!(height_tester(&[0xff, 0], *flags), Ok(0));

        // leading zeros are somtimes necessary to make values positive
        assert_eq!(height_tester(&[0, 0xff], *flags), Ok(0xff));
        // but are stripped when they are redundant
        if (flags & STRICT_MODE) != 0 {
            assert_eq!(
                height_tester(&[0, 0, 0, 0xff], *flags).unwrap_err().1,
                ErrorCode::AssertHeightAbsolute
            );
            assert_eq!(
                height_tester(&[0, 0, 0, 0x80], *flags).unwrap_err().1,
                ErrorCode::AssertHeightAbsolute
            );
            assert_eq!(
                height_tester(&[0, 0, 0, 0x7f], *flags).unwrap_err().1,
                ErrorCode::AssertHeightAbsolute
            );
            assert_eq!(
                height_tester(&[0, 0, 0], *flags).unwrap_err().1,
                ErrorCode::AssertHeightAbsolute
            );
            assert_eq!(
                height_tester(&[0], *flags).unwrap_err().1,
                ErrorCode::AssertHeightAbsolute
            );
        } else {
            assert_eq!(height_tester(&[0, 0, 0, 0xff], *flags), Ok(0xff));
            assert_eq!(height_tester(&[0, 0, 0, 0x80], *flags), Ok(0x80));
            assert_eq!(height_tester(&[0, 0, 0, 0x7f], *flags), Ok(0x7f));
            assert_eq!(height_tester(&[0, 0, 0], *flags), Ok(0));
            assert_eq!(height_tester(&[0], *flags), Ok(0));
        }

        // heights aren't allowed to be > 2^32 (i.e. 5 bytes)
        assert_eq!(
            height_tester(&[0x7f, 0xff, 0xff, 0xff, 0xff, 0xff], *flags)
                .unwrap_err()
                .1,
            ErrorCode::AssertHeightAbsolute
        );

        // this is small enough though
        assert_eq!(
            height_tester(&[0, 0xff, 0xff, 0xff, 0xff], *flags),
            Ok(0xffffffff)
        );

        let mut a = Allocator::new();
        let pair = a.new_pair(a.null(), a.null()).unwrap();
        let mut range_cache = RangeSet::new();
        assert_eq!(
            parse_height(
                &mut a,
                pair,
                ErrorCode::AssertHeightAbsolute,
                &mut range_cache,
                *flags
            ),
            Err(ValidationErr(pair, ErrorCode::AssertHeightAbsolute))
        );
    }
}

#[cfg(test)]
fn seconds_tester(buf: &[u8], flags: u32) -> Result<u64, ValidationErr> {
    let mut a = Allocator::new();
    let n = a.new_atom(buf).unwrap();
    let mut range_cache = RangeSet::new();

    parse_seconds(
        &mut a,
        n,
        ErrorCode::AssertSecondsAbsolute,
        &mut range_cache,
        flags,
    )
}

#[test]
fn test_parse_seconds() {
    for flags in &[0, STRICT_MODE] {
        // negative seconds can be ignored
        assert_eq!(seconds_tester(&[0x80], *flags), Ok(0));
        assert_eq!(seconds_tester(&[0xff], *flags), Ok(0));
        assert_eq!(seconds_tester(&[0xff, 0], *flags), Ok(0));

        // leading zeros are somtimes necessary to make values positive
        assert_eq!(seconds_tester(&[0, 0xff], *flags), Ok(0xff));
        // but are stripped when they are redundant
        if (flags & STRICT_MODE) != 0 {
            assert_eq!(
                seconds_tester(&[0, 0, 0, 0xff], *flags).unwrap_err().1,
                ErrorCode::AssertSecondsAbsolute
            );
            assert_eq!(
                seconds_tester(&[0, 0, 0, 0x80], *flags).unwrap_err().1,
                ErrorCode::AssertSecondsAbsolute
            );
            assert_eq!(
                seconds_tester(&[0, 0, 0, 0x7f], *flags).unwrap_err().1,
                ErrorCode::AssertSecondsAbsolute
            );
            assert_eq!(
                seconds_tester(&[0, 0, 0], *flags).unwrap_err().1,
                ErrorCode::AssertSecondsAbsolute
            );
            assert_eq!(
                seconds_tester(&[0], *flags).unwrap_err().1,
                ErrorCode::AssertSecondsAbsolute
            );
        } else {
            assert_eq!(seconds_tester(&[0, 0, 0, 0xff], *flags), Ok(0xff));
            assert_eq!(seconds_tester(&[0, 0, 0, 0x80], *flags), Ok(0x80));
            assert_eq!(seconds_tester(&[0, 0, 0, 0x7f], *flags), Ok(0x7f));
            assert_eq!(seconds_tester(&[0, 0, 0], *flags), Ok(0));
            assert_eq!(seconds_tester(&[0], *flags), Ok(0));
        }

        // seconds aren't allowed to be > 2^64 (i.e. 9 bytes)
        assert_eq!(
            seconds_tester(
                &[0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
                *flags
            )
            .unwrap_err()
            .1,
            ErrorCode::AssertSecondsAbsolute
        );

        // this is small enough though
        assert_eq!(
            seconds_tester(&[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff], *flags),
            Ok(0xffffffffffffffff)
        );

        let mut a = Allocator::new();
        let pair = a.new_pair(a.null(), a.null()).unwrap();
        let mut range_cache = RangeSet::new();
        assert_eq!(
            parse_seconds(
                &mut a,
                pair,
                ErrorCode::AssertSecondsAbsolute,
                &mut range_cache,
                *flags
            ),
            Err(ValidationErr(pair, ErrorCode::AssertSecondsAbsolute))
        );
    }
}

pub fn u64_from_bytes(buf: &[u8]) -> u64 {
    if buf.is_empty() {
        return 0;
    }

    let mut ret: u64 = 0;
    for b in buf {
        ret <<= 8;
        ret |= *b as u64;
    }
    ret
}

#[test]
fn test_u64_from_bytes() {
    assert_eq!(u64_from_bytes(&[]), 0);
    assert_eq!(u64_from_bytes(&[0xcc]), 0xcc);
    assert_eq!(u64_from_bytes(&[0xcc, 0x55]), 0xcc55);
    assert_eq!(u64_from_bytes(&[0xcc, 0x55, 0x88]), 0xcc5588);
    assert_eq!(u64_from_bytes(&[0xcc, 0x55, 0x88, 0xf3]), 0xcc5588f3);

    assert_eq!(u64_from_bytes(&[0xff]), 0xff);
    assert_eq!(u64_from_bytes(&[0xff, 0xff]), 0xffff);
    assert_eq!(u64_from_bytes(&[0xff, 0xff, 0xff]), 0xffffff);
    assert_eq!(u64_from_bytes(&[0xff, 0xff, 0xff, 0xff]), 0xffffffff);

    assert_eq!(u64_from_bytes(&[0x00]), 0);
    assert_eq!(u64_from_bytes(&[0x00, 0x00]), 0);
    assert_eq!(u64_from_bytes(&[0x00, 0xcc, 0x55, 0x88]), 0xcc5588);
    assert_eq!(u64_from_bytes(&[0x00, 0x00, 0xcc, 0x55, 0x88]), 0xcc5588);
    assert_eq!(u64_from_bytes(&[0x00, 0xcc, 0x55, 0x88, 0xf3]), 0xcc5588f3);

    assert_eq!(
        u64_from_bytes(&[0xcc, 0x55, 0x88, 0xf3, 0xcc, 0x55, 0x88, 0xf3]),
        0xcc5588f3cc5588f3
    );
}
