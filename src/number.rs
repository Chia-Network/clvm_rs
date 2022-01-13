#[cfg(not(feature = "num-bigint"))]
pub use crate::number_gmp::{Number, Sign};

#[cfg(feature = "num-bigint")]
pub use num_bigint::BigInt as Number;
#[cfg(feature = "num-bigint")]
pub use num_bigint::Sign;

use crate::allocator::{Allocator, NodePtr};
use crate::reduction::EvalErr;

pub fn ptr_from_number(allocator: &mut Allocator, item: &Number) -> Result<NodePtr, EvalErr> {
    let bytes: Vec<u8> = item.to_signed_bytes_be();
    allocator.new_atom(bytes.as_slice())
}

#[cfg(test)]
#[cfg(feature = "num-bigint")]
impl crate::number_traits::TestNumberTraits for Number {
    fn from_str_radix(mut s: &str, radix: i32) -> Number {
        num_traits::Num::from_str_radix(s, radix as u32).unwrap()
    }
}

#[cfg(feature = "num-bigint")]
impl crate::number_traits::NumberTraits for Number {
    fn from_unsigned_bytes_be(v: &[u8]) -> Number {
        let i = num_bigint::BigUint::from_bytes_be(v);
        i.into()
    }

    fn zero() -> Number {
        <Number as num_traits::Zero>::zero()
    }

    fn from_u8(v: &[u8]) -> Number {
        let len = v.len();
        if len == 0 {
            Number::zero()
        } else {
            Number::from_signed_bytes_be(v)
        }
    }

    fn to_u64(n: &Number) -> u64 {
        n.into()
    }

    fn div_mod_floor(&self, denominator: &Number) -> (Number, Number) {
        self.div_mod_floor(denominator)
    }

    fn mod_floor(&self, denominator: &Number) -> Number {
        num_integer::Integer::mod_floor(&self, denominator)
    }
}

#[test]
fn test_ptr_from_number() {
    use crate::number_traits::NumberTraits;
    let mut a = Allocator::new();

    // 0 is encoded as an empty string
    let num = Number::from_u8(&[0]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "0");
    assert_eq!(a.atom(ptr).len(), 0);

    let num = Number::from_u8(&[1]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "1");
    assert_eq!(&[1], &a.atom(ptr));

    // leading zeroes are redundant
    let num = Number::from_u8(&[0, 0, 0, 1]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "1");
    assert_eq!(&[1], &a.atom(ptr));

    let num = Number::from_u8(&[0x00, 0x00, 0x80]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "128");
    assert_eq!(&[0x00, 0x80], &a.atom(ptr));

    // A leading zero is necessary to encode a positive number with the
    // penultimate byte's most significant bit set
    let num = Number::from_u8(&[0x00, 0xff]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "255");
    assert_eq!(&[0x00, 0xff], &a.atom(ptr));

    let num = Number::from_u8(&[0x7f, 0xff]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "32767");
    assert_eq!(&[0x7f, 0xff], &a.atom(ptr));

    // the first byte is redundant, it's still -1
    let num = Number::from_u8(&[0xff, 0xff]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "-1");
    assert_eq!(&[0xff], &a.atom(ptr));

    let num = Number::from_u8(&[0xff]);
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "-1");
    assert_eq!(&[0xff], &a.atom(ptr));

    let num = Number::from_u8(&[0x00, 0x80, 0x00]);
    assert_eq!(format!("{}", num), "32768");
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(&[0x00, 0x80, 0x00], &a.atom(ptr));

    let num = Number::from_u8(&[0x00, 0x40, 0x00]);
    assert_eq!(format!("{}", num), "16384");
    let ptr = ptr_from_number(&mut a, &num).unwrap();
    assert_eq!(&[0x40, 0x00], &a.atom(ptr));
}

