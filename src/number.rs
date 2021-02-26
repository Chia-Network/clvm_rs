use crate::allocator::Allocator;
use crate::node::Node;
use crate::reduction::EvalErr;
use gmp::mpz::Mpz;

pub type Number = Mpz;
pub type Sign = gmp::sign::Sign;

// The two's complement is calculated by inverting the bits and
// adding one.
fn twos_complement(b: &[u8]) -> Vec<u8> {
    let mut ret = Vec::<u8>::new();
    ret.resize(b.len(), 0);
    let mut carry = 1;
    for i in (0..b.len()).rev() {
        ret[i] = (!b[i]).wrapping_add(carry);
        if carry == 1 {
            carry = if ret[i] == 0 { 1 } else { 0 };
        }
    }
    ret
}

#[test]
fn test_twos_complement() {
    fn test(b1: &[u8], b2: &[u8]) {
        assert_eq!(twos_complement(b1), b2);
    }

    test(&[0x00], &[0x00]);
    test(&[0x01], &[0xff]);
    test(&[0x02], &[0xfe]);
    test(&[0x80], &[0x80]);
    test(&[0xff, 0xff], &[0x00, 0x01]);
    test(&[0x80, 0x00, 0x00], &[0x80, 0x00, 0x00]);
    test(&[0x70, 0x00, 0x00], &[0x90, 0x00, 0x00]);
    test(&[0x10, 0x10, 0x10], &[0xef, 0xef, 0xf0]);
    test(&[0xff, 0xff, 0xff], &[0x00, 0x00, 0x01]);
    test(&[0xff, 0xff, 0xff, 0xff], &[0x00, 0x00, 0x00, 0x01]);
}

pub fn ones_complement(v: Number) -> Number {
    match v.sign() {
        Sign::Zero => -Number::one(),
        _ => -v - 1,
    }
}

#[test]
fn test_ones_complement() {
    fn test(val1: &str, val2: &str) {
        assert_eq!(
            ones_complement(Number::from_str_radix(val1, 10).unwrap()),
            Number::from_str_radix(val2, 10).unwrap()
        );
    }

    test("-1", "0");
    test("0", "-1");
    test("128", "-129");
    test("-129", "128");
    test("-65535", "65534");
    test("65534", "-65535");
}

fn number_to_signed_u8(v: &Number) -> Vec<u8> {
    match v.sign() {
        Sign::Positive => {
            let mut ret = Vec::<u8>::from(v);
            if ret[0] & 0x80 != 0 {
                // we have to prepend a zero-byte here, to preserve the positive
                // sign
                ret.insert(0, 0);
            }
            ret
        }
        Sign::Zero => vec![],
        Sign::Negative => {
            let mut ret = twos_complement(Vec::<u8>::from(v).as_slice());
            if ret[0] & 0x80 == 0 {
                // we have to prepend a sign-byte here, to preserve the negative
                // sign
                ret.insert(0, 0xff);
            }
            ret
        }
    }
}

#[test]
fn test_to_signed_bytes() {
    fn test(b: &[u8], val: &str, base: u8) {
        assert_eq!(
            number_to_signed_u8(&Number::from_str_radix(val, base).unwrap()),
            b
        );
    }
    test(&[0xff], "-1", 10);
    test(&[0xff, 0x00], "-256", 10);
    test(&[0x01, 0x00], "256", 10);
    test(&[0xff, 0x7f], "-129", 10);
    test(&[0x7f, 0xff], "7fff", 16);
    test(&[0x80, 0x01], "-7fff", 16);
    test(&[0x80, 0x00], "-8000", 16);
    test(&[0x00, 0xff, 0xff], "ffff", 16);
}

// generates big endian unsigned bytes representation of the number, dropping
// the sign.
pub fn number_to_unsigned_u8(v: &Number) -> Vec<u8> {
    Vec::<u8>::from(v)
}

#[test]
fn test_to_unsigned_u8() {
    fn test(b: &[u8], val: &str, base: u8) {
        assert_eq!(
            number_to_unsigned_u8(&Number::from_str_radix(val, base).unwrap()),
            b
        );
    }

    // number_to_unsigned_u8 drops the sign!
    // the most significant bit does not mean negative
    test(&[0x01], "-1", 10);
    test(&[0x01, 0x00], "-256", 10);
    test(&[0x01, 0x00], "256", 10);
    test(&[0xff, 0xff], "ffff", 16);
    test(&[0x7f, 0xff], "7fff", 16);
    test(&[0x80, 0x00], "-8000", 16);
    test(&[0x80, 0x00], "8000", 16);
    test(
        &[
            0x73, 0xED, 0xA7, 0x53, 0x29, 0x9D, 0x7D, 0x48, 0x33, 0x39, 0xD8, 0x08, 0x09, 0xA1,
            0xD8, 0x05, 0x53, 0xBD, 0xA4, 0x02, 0xFF, 0xFE, 0x5B, 0xFE, 0xFF, 0xFF, 0xFF, 0xFF,
            0x00, 0x00, 0x00, 0x01,
        ],
        "73EDA753299D7D483339D80809A1D80553BDA402FFFE5BFEFFFFFFFF00000001",
        16,
    );
}

pub fn ptr_from_number<T: Allocator>(
    allocator: &mut T,
    item: &Number,
) -> Result<T::Ptr, EvalErr<T::Ptr>> {
    let bytes: Vec<u8> = number_to_signed_u8(item);
    let mut slice = bytes.as_slice();

    // make number minimal by removing leading zeros
    while (!slice.is_empty()) && (slice[0] == 0) {
        if slice.len() > 1 && (slice[1] & 0x80 == 0x80) {
            break;
        }
        slice = &slice[1..];
    }
    allocator.new_atom(&slice)
}

impl<T: Allocator> From<&Node<'_, T>> for Option<Number> {
    fn from(item: &Node<T>) -> Self {
        let v: &[u8] = &item.atom()?;
        Some(number_from_signed_u8(v))
    }
}

pub fn number_from_signed_u8(v: &[u8]) -> Number {
    let len = v.len();
    if len == 0 {
        Number::zero()
    } else if v[0] & 0x80 != 0 {
        // the number is negative
        -Number::from(twos_complement(v).as_slice())
    } else {
        Number::from(v)
    }
}

#[test]
fn test_number_from_signed_u8() {
    fn test(b: &[u8], val: &str, base: u8) {
        assert_eq!(
            number_from_signed_u8(b),
            Number::from_str_radix(val, base).unwrap()
        );
    }

    test(&[0xff], "-1", 10);

    // the first byte is redundant, it's still -1
    test(&[0xff, 0xff, 0xff], "-1", 10);

    test(&[0xff, 0x00], "-256", 10);
    test(&[0x01, 0x00], "256", 10);

    // leading zeroes are redundant
    test(&[0x00, 0xff, 0xff], "ffff", 16);
    test(&[0x7f, 0xff], "7fff", 16);
    test(&[0x80, 0x01], "-7fff", 16);
    test(&[0x80, 0x00], "-8000", 16);
    test(&[0x00, 0x80, 0x00], "8000", 16);
}

pub fn number_from_unsigned_u8(v: &[u8]) -> Number {
    let len = v.len();
    if len == 0 {
        Number::zero()
    } else {
        Number::from(v)
    }
}

#[test]
fn test_number_from_unsigned_u8() {
    fn test(b: &[u8], val: &str, base: u8) {
        assert_eq!(
            number_from_unsigned_u8(b),
            Number::from_str_radix(val, base).unwrap()
        );
    }

    test(&[0xff], "ff", 16);
    test(&[0x7f], "7f", 16);
    test(&[0x80, 0x7f], "807f", 16);
    test(&[0xff, 0xff], "ffff", 16);
    test(&[0x00, 0x01], "1", 16);
}

#[cfg(test)]
use crate::int_allocator::IntAllocator;

#[test]
fn test_ptr_from_number() {
    fn test(b: &[u8], val: &str) {
        let mut a = IntAllocator::new();
        let num = Number::from_str_radix(val, 10).unwrap();
        let ptr = ptr_from_number(&mut a, &num).unwrap();
        assert_eq!(&b, &a.atom(&ptr));
    }

    // 0 is encoded as an empty string
    test(&[], "0");
    test(&[1], "1");
    test(&[0xff], "-1");

    // A leading zero is necessary to encode a positive number with the
    // penultimate byte's most significant bit set
    test(&[0, 0x80], "128");
    test(&[0, 0xff], "255");
    test(&[0x7f, 0xff], "32767");
    test(&[0x00, 0x80, 0x00], "32768");
    test(&[0x40, 0x00], "16384");
}
