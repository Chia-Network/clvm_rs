use crate::gmp_ffi as gmp;
use crate::node::Node;
use core::mem::MaybeUninit;
use std::cmp::Ordering;
use std::cmp::PartialOrd;
use std::ffi::c_void;
use std::ops::Drop;
use std::ops::{
    AddAssign, BitAndAssign, BitOrAssign, BitXorAssign, MulAssign, Not, Shl, Shr, SubAssign,
};
use crate::number_traits::NumberTraits;
#[cfg(test)]
use crate::number_traits::TestNumberTraits;

#[allow(clippy::enum_variant_names)]
#[derive(PartialEq)]
pub enum Sign {
    Minus,
    NoSign,
    Plus,
}

pub struct Number {
    v: gmp::mpz_t,
}

#[cfg(test)]
impl TestNumberTraits for Number {
    fn from_str_radix(mut s: &str, radix: i32) -> Number {
        let negative = s.get(0..1).unwrap() == "-";
        if negative {
            s = s.get(1..).unwrap();
        }
        let input = CString::new(s).unwrap();
        let mut v = MaybeUninit::<gmp::mpz_t>::uninit();
        let result = unsafe { gmp::mpz_init_set_str(v.as_mut_ptr(), input.as_ptr(), radix) };
        // v will be initialized even if an error occurs, so we will need to
        // capture it in a Number regardless
        let mut ret = Number {
            v: unsafe { v.assume_init() },
        };
        if negative {
            unsafe {
                gmp::mpz_neg(&mut ret.v, &ret.v);
            }
        }
        assert!(result == 0);
        ret
    }
}

impl NumberTraits for Number {
    fn from_unsigned_bytes_be(v: &[u8]) -> Number {
        let mut ret = Number::zero();
        if !v.is_empty() {
            unsafe {
                gmp::mpz_import(&mut ret.v, v.len(), 1, 1, 0, 0, v.as_ptr() as *const c_void);
            }
        }
        ret
    }

    fn zero() -> Number {
        let mut v = MaybeUninit::<gmp::mpz_t>::uninit();
        unsafe {
            gmp::mpz_init(v.as_mut_ptr());
        }
        Number {
            v: unsafe { v.assume_init() },
        }
    }

    fn from_u8(v: &[u8]) -> Number {
        Number::from_signed_bytes_be(v)
    }

    fn to_u64(n: &Number) -> u64 {
        n.into()
    }

    // returns the quotient and remained, from dividing self with denominator
    fn div_mod_floor(&self, denominator: &Number) -> (Number, Number) {
        let mut q = Number::zero();
        let mut r = Number::zero();
        unsafe {
            gmp::mpz_fdiv_qr(&mut q.v, &mut r.v, &self.v, &denominator.v);
        }
        (q, r)
    }

    fn mod_floor(&self, denominator: &Number) -> Number {
        let mut r = Number::zero();
        unsafe {
            gmp::mpz_fdiv_r(&mut r.v, &self.v, &denominator.v);
        }
        r
    }
}

impl Number {
    pub fn from_signed_bytes_be(v: &[u8]) -> Number {
        let mut ret = Number::zero();
        if v.is_empty() {
            return ret;
        }
        // mpz_import() only reads unsigned values
        let negative = (v[0] & 0x80) != 0;

        if negative {
            // since the bytes we read are two's complement
            // if the most significant bit was set, we need to
            // convert the value to a negative one. We do this by flipping
            // all bits, adding one and then negating it.
            let mut v = v.to_vec();
            for digit in &mut v {
                *digit = !*digit;
            }
            unsafe {
                gmp::mpz_import(&mut ret.v, v.len(), 1, 1, 0, 0, v.as_ptr() as *const c_void);
                gmp::mpz_add_ui(&mut ret.v, &ret.v, 1);
                gmp::mpz_neg(&mut ret.v, &ret.v);
            }
        } else {
            unsafe {
                gmp::mpz_import(&mut ret.v, v.len(), 1, 1, 0, 0, v.as_ptr() as *const c_void);
            }
        }
        ret
    }

    pub fn to_signed_bytes_be(&self) -> Vec<u8> {
        let size = (self.bits() + 7) / 8;
        let mut ret: Vec<u8> = Vec::new();
        if size == 0 {
            return ret;
        }
        ret.resize(size + 1, 0);
        let sign = self.sign();
        let mut out_size: usize = size;
        unsafe {
            gmp::mpz_export(
                ret.as_mut_slice()[1..].as_mut_ptr() as *mut c_void,
                &mut out_size,
                1,
                1,
                0,
                0,
                &self.v,
            );
        }
        // apparently mpz_export prints 0 bytes to the buffer if the value is 0
        // hence the special case in the assert below.
        assert!(out_size == ret.len() - 1);
        if sign == Sign::Minus {
            // If the value is negative, we need to convert it to two's
            // complement. We can't do that in-place.
            let mut carry = true;
            for digit in &mut ret.iter_mut().rev() {
                let res = (!*digit).overflowing_add(carry as u8);
                *digit = res.0;
                carry = res.1;
            }
            assert!(!carry);
            assert!(ret[0] & 0x80 != 0);
            if (ret[1] & 0x80) != 0 {
                ret.remove(0);
            }
        } else if ret[1] & 0x80 == 0 {
            ret.remove(0);
        }
        ret
    }

    pub fn to_bytes_le(&self) -> (Sign, Vec<u8>) {
        let sgn = self.sign();

        let size = (self.bits() + 7) / 8;
        let mut ret: Vec<u8> = Vec::new();
        if size == 0 {
            return (Sign::NoSign, ret);
        }
        ret.resize(size, 0);

        let mut out_size: usize = size;
        unsafe {
            gmp::mpz_export(
                ret.as_mut_ptr() as *mut c_void,
                &mut out_size,
                -1,
                1,
                0,
                0,
                &self.v,
            );
        }
        assert_eq!(out_size, ret.len());
        (sgn, ret)
    }

    pub fn bits(&self) -> usize {
        // GnuMP says that any integer needs at least 1 bit to be represented.
        // but we say 0 requires 0 bits
        if self.sign() == Sign::NoSign {
            0
        } else {
            unsafe { gmp::mpz_sizeinbase(&self.v, 2) }
        }
    }

    pub fn sign(&self) -> Sign {
        match unsafe { gmp::mpz_cmp_si(&self.v, 0) } {
            d if d < 0 => Sign::Minus,
            d if d > 0 => Sign::Plus,
            _ => Sign::NoSign,
        }
    }

    pub fn div_floor(&self, denominator: &Number) -> Number {
        let mut ret = Number::zero();
        unsafe {
            gmp::mpz_fdiv_q(&mut ret.v, &self.v, &denominator.v);
        }
        ret
    }
}

impl Drop for Number {
    fn drop(&mut self) {
        unsafe {
            gmp::mpz_clear(&mut self.v);
        }
    }
}

// Addition

impl AddAssign<&Number> for Number {
    fn add_assign(&mut self, other: &Self) {
        unsafe {
            gmp::mpz_add(&mut self.v, &self.v, &other.v);
        }
    }
}

// This is only here for op_div()
impl AddAssign<u64> for Number {
    fn add_assign(&mut self, other: u64) {
        unsafe {
            gmp::mpz_add_ui(&mut self.v, &self.v, other);
        }
    }
}

// Subtraction

impl SubAssign<&Number> for Number {
    fn sub_assign(&mut self, other: &Self) {
        unsafe {
            gmp::mpz_sub(&mut self.v, &self.v, &other.v);
        }
    }
}

// Multiplication

impl MulAssign<Number> for Number {
    fn mul_assign(&mut self, other: Self) {
        unsafe {
            gmp::mpz_mul(&mut self.v, &self.v, &other.v);
        }
    }
}

// Shift

impl Shl<i32> for Number {
    type Output = Self;
    fn shl(mut self, n: i32) -> Self {
        assert!(n >= 0);
        unsafe {
            gmp::mpz_mul_2exp(&mut self.v, &self.v, n as u64);
        }
        self
    }
}

impl Shr<i32> for Number {
    type Output = Self;
    fn shr(mut self, n: i32) -> Self {
        assert!(n >= 0);
        unsafe {
            gmp::mpz_fdiv_q_2exp(&mut self.v, &self.v, n as u64);
        }
        self
    }
}

// Conversion

impl From<i64> for Number {
    fn from(other: i64) -> Self {
        let mut v = MaybeUninit::<gmp::mpz_t>::uninit();
        unsafe {
            gmp::mpz_init_set_si(v.as_mut_ptr(), other);
        }
        Number {
            v: unsafe { v.assume_init() },
        }
    }
}

impl From<i32> for Number {
    fn from(other: i32) -> Self {
        let mut v = MaybeUninit::<gmp::mpz_t>::uninit();
        unsafe {
            gmp::mpz_init_set_si(v.as_mut_ptr(), other as i64);
        }
        Number {
            v: unsafe { v.assume_init() },
        }
    }
}

impl From<u64> for Number {
    fn from(other: u64) -> Self {
        let mut v = MaybeUninit::<gmp::mpz_t>::uninit();
        unsafe {
            gmp::mpz_init_set_ui(v.as_mut_ptr(), other);
        }
        Number {
            v: unsafe { v.assume_init() },
        }
    }
}

impl From<usize> for Number {
    fn from(other: usize) -> Self {
        let mut v = MaybeUninit::<gmp::mpz_t>::uninit();
        unsafe {
            gmp::mpz_init_set_ui(v.as_mut_ptr(), other as u64);
        }
        Number {
            v: unsafe { v.assume_init() },
        }
    }
}

impl From<Number> for u64 {
    fn from(n: Number) -> u64 {
        unsafe {
            assert!(gmp::mpz_sizeinbase(&n.v, 2) <= 64);
            assert!(gmp::mpz_cmp_si(&n.v, 0) >= 0);
            gmp::mpz_get_ui(&n.v)
        }
    }
}

impl From<Number> for i64 {
    fn from(n: Number) -> i64 {
        unsafe {
            assert!(gmp::mpz_sizeinbase(&n.v, 2) <= 64);
            gmp::mpz_get_si(&n.v)
        }
    }
}

// Bit operations

impl BitXorAssign<&Number> for Number {
    fn bitxor_assign(&mut self, other: &Self) {
        unsafe {
            gmp::mpz_xor(&mut self.v, &self.v, &other.v);
        }
    }
}

impl BitOrAssign<&Number> for Number {
    fn bitor_assign(&mut self, other: &Self) {
        unsafe {
            gmp::mpz_ior(&mut self.v, &self.v, &other.v);
        }
    }
}

impl BitAndAssign<&Number> for Number {
    fn bitand_assign(&mut self, other: &Self) {
        unsafe {
            gmp::mpz_and(&mut self.v, &self.v, &other.v);
        }
    }
}

impl Not for Number {
    type Output = Self;
    fn not(self) -> Self {
        let mut ret = Number::zero();
        unsafe {
            gmp::mpz_com(&mut ret.v, &self.v);
        }
        ret
    }
}

// Comparisons

impl PartialEq<Number> for Number {
    fn eq(&self, other: &Self) -> bool {
        unsafe { gmp::mpz_cmp(&self.v, &other.v) == 0 }
    }
}

impl PartialEq<u64> for Number {
    fn eq(&self, other: &u64) -> bool {
        unsafe { gmp::mpz_cmp_ui(&self.v, *other) == 0 }
    }
}

impl PartialEq<i64> for Number {
    fn eq(&self, other: &i64) -> bool {
        unsafe { gmp::mpz_cmp_si(&self.v, *other) == 0 }
    }
}

impl PartialEq<i32> for Number {
    fn eq(&self, other: &i32) -> bool {
        unsafe { gmp::mpz_cmp_si(&self.v, *other as i64) == 0 }
    }
}

fn ord_helper(r: i32) -> Option<Ordering> {
    match r {
        d if d < 0 => Some(Ordering::Less),
        d if d > 0 => Some(Ordering::Greater),
        _ => Some(Ordering::Equal),
    }
}

impl PartialOrd<Number> for Number {
    fn partial_cmp(&self, other: &Number) -> Option<Ordering> {
        ord_helper(unsafe { gmp::mpz_cmp(&self.v, &other.v) })
    }
}

impl PartialOrd<u64> for Number {
    fn partial_cmp(&self, other: &u64) -> Option<Ordering> {
        ord_helper(unsafe { gmp::mpz_cmp_ui(&self.v, *other) })
    }
}

unsafe impl Sync for Number {}

impl From<&Node<'_>> for Option<Number> {
    fn from(item: &Node) -> Self {
        let v: &[u8] = item.atom()?;
        Some(Number::from_u8(v))
    }
}

// ==== TESTS ====

#[cfg(test)]
use std::ffi::{CStr, CString};
#[cfg(test)]
use std::fmt;

#[cfg(test)]
impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let len = unsafe { gmp::mpz_sizeinbase(&self.v, 10) } + 2;
        let mut storage = Vec::<i8>::with_capacity(len);
        let c_str = unsafe { gmp::mpz_get_str(storage.as_mut_ptr(), 10, &self.v) };
        unsafe { f.write_str(CStr::from_ptr(c_str).to_str().unwrap()) }
    }
}

#[cfg(test)]
fn roundtrip_bytes(b: &[u8]) {
    let negative = b.len() > 0 && (b[0] & 0x80) != 0;
    let zero = b.len() == 0 || (b.len() == 1 && b[0] == 0);

    {
        let num = Number::from_signed_bytes_be(b);

        if negative {
            assert!(num.sign() == Sign::Minus);
        } else if zero {
            assert!(num.sign() == Sign::NoSign);
        } else {
            assert!(num.sign() == Sign::Plus);
        }

        let round_trip = num.to_signed_bytes_be();

        assert_eq!(round_trip, b);

        // test to_bytes_le()
        let (sign, mut buf_le) = num.to_bytes_le();

        assert!(sign == num.sign());

        // the buffer we get from to_bytes_le() is unsigned (since the sign is
        // returned separately). This means it doesn't ever need to prepend a 0
        // byte when the MSB is set. When we're comparing this against the input
        // buffer, we need to add such 0 byte to buf_le to make them compare
        // equal.
        // the 0 prefix has to be added to the end though, since it's little
        // endian
        if buf_le.len() > 0 && (buf_le.last().unwrap() & 0x80) != 0 {
            buf_le.push(0);
        }

        if sign != Sign::Minus {
            assert!(buf_le.iter().eq(b.iter().rev()));
        } else {
            let mut negated = Number::zero();
            unsafe {
                gmp::mpz_neg(&mut negated.v, &num.v);
            }
            let magnitude = negated.to_signed_bytes_be();
            assert!(buf_le.iter().eq(magnitude.iter().rev()));
        }
    }

    // test parsing unsigned bytes
    {
        let unsigned_num = Number::from_unsigned_bytes_be(b);
        assert!(unsigned_num.sign() != Sign::Minus);
        let unsigned_round_trip = unsigned_num.to_signed_bytes_be();
        let unsigned_round_trip = if unsigned_round_trip == &[0] {
            &unsigned_round_trip[1..]
        } else {
            &unsigned_round_trip
        };
        if b.len() > 0 && (b[0] & 0x80) != 0 {
            // we expect a new leading zero here, to keep the value positive
            assert!(unsigned_round_trip[0] == 0);
            assert_eq!(&unsigned_round_trip[1..], b);
        } else {
            assert_eq!(unsigned_round_trip, b);
        }
    }
}

#[test]
fn test_number_round_trip_bytes() {
    roundtrip_bytes(&[]);

    // 0 doesn't round-trip, since we represent that by an empty buffer
    for i in 1..=255 {
        roundtrip_bytes(&[i]);
    }

    for i in 0..=127 {
        roundtrip_bytes(&[0xff, i]);
    }

    for i in 128..=255 {
        roundtrip_bytes(&[0, i]);
    }

    for i in 0..=127 {
        roundtrip_bytes(&[
            0xff, i, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        ]);
    }

    for i in 128..=255 {
        roundtrip_bytes(&[
            0, i, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        ]);
    }

    for i in 0..=127 {
        roundtrip_bytes(&[0xff, i, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    for i in 128..=255 {
        roundtrip_bytes(&[0, i, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }
}

#[cfg(test)]
fn roundtrip_u64(v: u64) {
    let num: Number = v.into();
    assert!(num.sign() != Sign::Minus);

    assert!(num.bits() <= 64);
    assert!(!(num < v));
    assert!(!(num > v));
    assert!(!(num != v));
    assert!(num == v);
    assert!(num <= v);
    assert!(num >= v);

    if v != u64::MAX {
        assert!(num < v + 1);
        assert!(!(num > v + 1));
        assert!(num != v + 1);
        assert!(!(num == v + 1));
        assert!(num <= v + 1);
        assert!(!(num >= v + 1));
    }

    if v != u64::MIN {
        assert!(!(num < v - 1));
        assert!(num > v - 1);
        assert!(num != v - 1);
        assert!(!(num == v - 1));
        assert!(!(num <= v - 1));
        assert!(num >= v - 1);
    }

    let round_trip: u64 = num.into();
    assert_eq!(round_trip, v);
}

#[test]
fn test_round_trip_u64() {
    for v in 0..=0x100 {
        roundtrip_u64(v);
    }

    for v in 0x7ffe..=0x8001 {
        roundtrip_u64(v);
    }

    for v in 0xfffe..=0x10000 {
        roundtrip_u64(v);
    }

    for v in 0x7ffffffe..=0x80000001 {
        roundtrip_u64(v);
    }
    for v in 0xfffffffe..=0x100000000 {
        roundtrip_u64(v);
    }

    for v in 0x7ffffffffffffffe..=0x8000000000000001 {
        roundtrip_u64(v);
    }

    for v in 0xfffffffffffffffe..=0xffffffffffffffff {
        roundtrip_u64(v);
    }
}

#[cfg(test)]
fn roundtrip_i64(v: i64) {
    let num: Number = v.into();
    if v == 0 {
        assert!(num.sign() == Sign::NoSign);
    } else if v < 0 {
        assert!(num.sign() == Sign::Minus);
    } else if v > 0 {
        assert!(num.sign() == Sign::Plus);
    }

    assert!(num.bits() <= 64);
    let round_trip: i64 = num.into();
    assert_eq!(round_trip, v);
}

#[test]
fn test_round_trip_i64() {
    for v in -0x100..=0x100 {
        roundtrip_i64(v);
    }

    for v in 0x7ffe..=0x8001 {
        roundtrip_i64(v);
    }

    for v in -0x8001..-0x7ffe {
        roundtrip_i64(v);
    }

    for v in 0xfffe..=0x10000 {
        roundtrip_i64(v);
    }

    for v in -0x10000..-0xfffe {
        roundtrip_i64(v);
    }

    for v in 0x7ffffffe..=0x80000001 {
        roundtrip_i64(v);
    }

    for v in -0x80000001..-0x7ffffffe {
        roundtrip_i64(v);
    }

    for v in 0xfffffffe..=0x100000000 {
        roundtrip_i64(v);
    }

    for v in -0x100000000..-0xfffffffe {
        roundtrip_i64(v);
    }

    for v in 0x7ffffffffffffffe..=0x7fffffffffffffff {
        roundtrip_i64(v);
    }

    for v in -0x8000000000000000..-0x7ffffffffffffffe {
        roundtrip_i64(v);
    }
}

#[cfg(test)]
fn bits(b: &[u8]) -> u64 {
    Number::from_signed_bytes_be(b).bits() as u64
}

#[test]
fn test_bits() {
    assert_eq!(bits(&[]), 0);
    assert_eq!(bits(&[0]), 0);
    assert_eq!(bits(&[0b01111111]), 7);
    assert_eq!(bits(&[0b00111111]), 6);
    assert_eq!(bits(&[0b00011111]), 5);
    assert_eq!(bits(&[0b00001111]), 4);
    assert_eq!(bits(&[0b00000111]), 3);
    assert_eq!(bits(&[0b00000011]), 2);
    assert_eq!(bits(&[0b00000001]), 1);
    assert_eq!(bits(&[0b00000000]), 0);

    assert_eq!(bits(&[0b01111111, 0xff]), 15);
    assert_eq!(bits(&[0b00111111, 0xff]), 14);
    assert_eq!(bits(&[0b00011111, 0xff]), 13);
    assert_eq!(bits(&[0b00001111, 0xff]), 12);
    assert_eq!(bits(&[0b00000111, 0xff]), 11);
    assert_eq!(bits(&[0b00000011, 0xff]), 10);
    assert_eq!(bits(&[0b00000001, 0xff]), 9);
    assert_eq!(bits(&[0b00000000, 0xff]), 8);

    assert_eq!(bits(&[0b11111111]), 1);
    assert_eq!(bits(&[0b11111110]), 2);
    assert_eq!(bits(&[0b11111100]), 3);
    assert_eq!(bits(&[0b11111000]), 4);
    assert_eq!(bits(&[0b11110000]), 5);
    assert_eq!(bits(&[0b11100000]), 6);
    assert_eq!(bits(&[0b11000000]), 7);
    assert_eq!(bits(&[0b10000000]), 8);

    assert_eq!(bits(&[0b11111111, 0]), 9);
    assert_eq!(bits(&[0b11111110, 0]), 10);
    assert_eq!(bits(&[0b11111100, 0]), 11);
    assert_eq!(bits(&[0b11111000, 0]), 12);
    assert_eq!(bits(&[0b11110000, 0]), 13);
    assert_eq!(bits(&[0b11100000, 0]), 14);
    assert_eq!(bits(&[0b11000000, 0]), 15);
    assert_eq!(bits(&[0b10000000, 0]), 16);
}
