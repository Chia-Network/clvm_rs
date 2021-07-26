use super::rangeset::RangeSet;
use super::validation_error::{atom, ErrorCode, ValidationErr};
use crate::allocator::{Allocator, AtomBuf, NodePtr, SExp};
use crate::run_program::STRICT_MODE;

fn count_zeros_impl(buf: &[u8]) -> usize {
    let mut ret: usize = 0;
    while ret < buf.len() && buf[ret] == 0 {
        ret += 1;
    }
    ret as usize
}

// this *just* counts zeros. Note that not all zeros are redundant. the last
// zero byte may be necessary. This has to be fixed up by the caller
fn count_zeros(a: &Allocator, range_cache: &mut RangeSet, atom_range: &AtomBuf) -> usize {
    if atom_range.is_empty() {
        return 0;
    }

    // early exit for common case
    if atom_range.len() < 20 {
        return count_zeros_impl(a.buf(atom_range));
    }

    let range = atom_range.idx_range();
    let ranges = range_cache.not_overlapping(range.0, range.1);

    // an empty range means the entire range we passed in is all zeros
    if ranges.is_empty() {
        return atom_range.len();
    }

    let full_buffer = a.buf(atom_range);

    let mut start = range.0;
    let mut ret = 0_usize;
    for r in ranges {
        debug_assert!(r.0 >= range.0);
        debug_assert!(r.1 <= range.1);
        debug_assert!(r.0 < r.1);

        if r.0 != start {
            // skip this range of known zero-bytes
            ret += (r.0 - start) as usize;
        }
        let buf = &full_buffer[(r.0 - range.0) as usize..(r.1 - range.0) as usize];
        let zeros = count_zeros_impl(buf);
        ret += zeros;
        if zeros < buf.len() {
            break;
        }
        start = r.1;
    }

    debug_assert!(start <= range.1);

    if ret > 0 {
        range_cache.add(range.0, range.0 + ret as u32);
    }
    ret
}

pub fn sanitize_uint<'a>(
    a: &'a Allocator,
    n: NodePtr,
    max_size: usize,
    code: ErrorCode,
    range_cache: &mut RangeSet,
    flags: u32,
) -> Result<&'a [u8], ValidationErr> {
    assert!(max_size <= 8);

    let buf = atom(a, n, code)?;

    if buf.is_empty() {
        return Ok(&[]);
    }

    // if the top bit is set, it's a negative number
    if (buf[0] & 0x80) != 0 {
        return Err(ValidationErr(n, ErrorCode::NegativeAmount));
    }

    let atom_range = match a.sexp(n) {
        SExp::Atom(b) => b,
        _ => {
            panic!("unreachable");
        }
    };

    // strip redundant leading zeros
    let mut i = count_zeros(a, range_cache, &atom_range);
    let len = buf.len();

    if i > 0 {
        let range = atom_range.idx_range();
        range_cache.add(range.0, range.0 + i as u32);
    }

    // if there are too many bytes left in the value, it's too big
    if len - i > max_size {
        return Err(ValidationErr(n, code));
    }

    if i > 0 && i < len && buf[i] >= 0x80 {
        // in this case, the last 0 byte isn't redundant, it's necessary to make
        // the integer positive.
        i -= 1;
    }

    if (flags & STRICT_MODE) != 0 && i > 0 {
        // in strict mode, we don't allow any redundant leading zeros
        return Err(ValidationErr(n, code));
    }

    Ok(&buf[i..len])
}

#[test]
fn test_count_zeros_impl() {
    // this function doesn't care whether the zero is redundant or not, it just
    // counts zeros
    assert_eq!(count_zeros_impl(&[0x80]), 0);
    assert_eq!(count_zeros_impl(&[]), 0);
    assert_eq!(count_zeros_impl(&[0]), 1);
    assert_eq!(count_zeros_impl(&[0, 0x80]), 1);
    assert_eq!(count_zeros_impl(&[0, 0, 0, 0, 0, 0, 0x7f]), 6);
    assert_eq!(count_zeros_impl(&[0, 0, 0, 0, 0, 0, 0x80]), 6);
    assert_eq!(count_zeros_impl(&[0xff, 0, 0, 0, 0, 0]), 0);
    assert_eq!(count_zeros_impl(&[0, 0xff, 0, 0, 0, 0, 0]), 1);
}

#[cfg(test)]
fn get_buf(a: &Allocator, n: NodePtr) -> AtomBuf {
    match a.sexp(n) {
        SExp::Atom(b) => b,
        SExp::Pair(_, _) => {
            panic!("not an atom");
        }
    }
}

#[test]
fn test_count_zeros_and_sanitize_int() {
    let mut a = Allocator::new();

    // any buffer < 20 bytes ignore the range cache, so we need a bigger buffer
    // than that.

    // start with one big buffer.
    let atom = {
        let mut buf = Vec::<u8>::new();
        for _i in 0..1024 {
            buf.push(0);
        }

        // make some of the bytes non-zero
        buf[0] = 0xff;
        buf[100] = 0x7f;
        buf[1023] = 0xff;
        a.new_atom(&buf)
    }
    .unwrap();

    // note that this one atom we just created with this buffer will not be
    // allocated at index 0 on the heap, but index 1. We pre-allocate the atom 1
    // at index 0. So the indices in the cache will appear shifted one step
    {
        let mut range_cache = RangeSet::new();
        let no_leading_zero = a.new_substr(atom, 0, 40).unwrap();
        let b = get_buf(&mut a, no_leading_zero);
        assert_eq!(count_zeros(&mut a, &mut range_cache, &b), 0);
        // since we didn't count any leading zeros, we also didn't add anything
        // to the cache
        assert_eq!(range_cache.not_overlapping(1, 41), vec![(1, 41)]);

        let just_zeros = a.new_substr(atom, 10, 70).unwrap();
        let b = get_buf(&mut a, just_zeros);
        assert_eq!(count_zeros(&mut a, &mut range_cache, &b), 60);
        // all of the range [10, 70) were zeros. That should have been added to
        // the cache
        assert_eq!(range_cache.not_overlapping(11, 71), vec![]);
        assert_eq!(
            range_cache.not_overlapping(10, 72),
            vec![(10, 11), (71, 72)]
        );

        let a1 = a.new_substr(atom, 1, 101).unwrap();
        let b = get_buf(&mut a, a1);
        assert_eq!(count_zeros(&mut a, &mut range_cache, &b), 99);
        // now we discovered that [1, 100) are zeros too, which should have been
        // added to the cache
        assert_eq!(range_cache.not_overlapping(1, 101), vec![(1, 2)]);
        assert_eq!(
            range_cache.not_overlapping(1, 102),
            vec![(1, 2), (101, 102)]
        );

        let a1 = a.new_substr(atom, 1, 101).unwrap();
        let b = get_buf(&mut a, a1);
        assert_eq!(count_zeros(&mut a, &mut range_cache, &b), 99);
        // this was fully a cache hit, nothing was added
        assert_eq!(range_cache.not_overlapping(1, 101), vec![(1, 2)]);

        // a new all-zeros range
        let a1 = a.new_substr(atom, 1000, 1024).unwrap();
        let b = get_buf(&mut a, a1);
        assert_eq!(count_zeros(&mut a, &mut range_cache, &b), 23);

        // all of the zeros are cache, the fact that the last zero is not
        // redundant is unimportant
        assert_eq!(range_cache.not_overlapping(1001, 1025), vec![(1024, 1025)]);
    }

    // now do the same thing again, but use sanitize_uint instead of
    // count_zeros()
    {
        let e = ErrorCode::InvalidCoinAmount;
        let mut range_cache = RangeSet::new();
        let no_leading_zero = a.new_substr(atom, 0, 8).unwrap();
        // this is a negative number, not allowed
        assert!(sanitize_uint(&a, no_leading_zero, 8, e, &mut range_cache, 0).is_err());
        // since we didn't count any leading zeros, we also didn't add anything
        // to the cache
        assert_eq!(range_cache.not_overlapping(1, 9), vec![(1, 9)]);

        let just_zeros = a.new_substr(atom, 10, 70).unwrap();
        // keep in mind that an empty range is the same as 0
        assert_eq!(
            sanitize_uint(&a, just_zeros, 8, e, &mut range_cache, 0).unwrap(),
            &[]
        );
        // all of the range [10, 70) were zeros. That should have been added to
        // the cache
        assert_eq!(range_cache.not_overlapping(11, 71), vec![]);
        assert_eq!(
            range_cache.not_overlapping(10, 72),
            vec![(10, 11), (71, 72)]
        );

        let a1 = a.new_substr(atom, 1, 101).unwrap();
        assert_eq!(
            sanitize_uint(&a, a1, 8, e, &mut range_cache, 0).unwrap(),
            &[0x7f]
        );
        // now we discovered that [1, 100) are zeros too, which should have been
        // added to the cache
        assert_eq!(range_cache.not_overlapping(1, 101), vec![(1, 2)]);
        assert_eq!(
            range_cache.not_overlapping(1, 102),
            vec![(1, 2), (101, 102)]
        );

        let a1 = a.new_substr(atom, 1, 101).unwrap();
        assert_eq!(
            sanitize_uint(&a, a1, 8, e, &mut range_cache, 0).unwrap(),
            &[0x7f]
        );
        // this was fully a cache hit, nothing was added
        assert_eq!(range_cache.not_overlapping(1, 101), vec![(1, 2)]);

        // a new all-zeros range
        let a1 = a.new_substr(atom, 1000, 1024).unwrap();
        assert_eq!(
            sanitize_uint(&a, a1, 8, e, &mut range_cache, 0).unwrap(),
            &[0, 0xff]
        );

        // all of the zeros are cache, the fact that the last zero is not
        // redundant is unimportant
        assert_eq!(range_cache.not_overlapping(1001, 1025), vec![(1024, 1025)]);
    }
}
