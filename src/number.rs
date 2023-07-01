use crate::allocator::{Allocator, NodePtr};
use crate::reduction::EvalErr;

use num_bigint::BigInt;
pub type Number = BigInt;

// This low-level conversion function is meant to be used by the Allocator, for
// logic interacting with the CLVM heap/allocator, use new_number() and number()
// instead.
pub fn node_from_number(allocator: &mut Allocator, item: &Number) -> Result<NodePtr, EvalErr> {
    let bytes: Vec<u8> = item.to_signed_bytes_be();
    let mut slice = bytes.as_slice();

    // make number minimal by removing leading zeros
    while (!slice.is_empty()) && (slice[0] == 0) {
        if slice.len() > 1 && (slice[1] & 0x80 == 0x80) {
            break;
        }
        slice = &slice[1..];
    }
    allocator.new_atom(slice)
}

// This low-level conversion function is meant to be used by the Allocator, for
// logic interacting with the CLVM heap/allocator, use new_number() and number()
// instead.
pub fn number_from_u8(v: &[u8]) -> Number {
    let len = v.len();
    if len == 0 {
        0.into()
    } else {
        Number::from_signed_bytes_be(v)
    }
}

#[test]
fn test_node_from_number() {
    let mut a = Allocator::new();

    // 0 is encoded as an empty string
    let num = number_from_u8(&[0]);
    let ptr = node_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "0");
    assert_eq!(a.atom(ptr).len(), 0);

    let num = number_from_u8(&[1]);
    let ptr = node_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "1");
    assert_eq!(&[1], &a.atom(ptr));

    // leading zeroes are redundant
    let num = number_from_u8(&[0, 0, 0, 1]);
    let ptr = node_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "1");
    assert_eq!(&[1], &a.atom(ptr));

    let num = number_from_u8(&[0x00, 0x00, 0x80]);
    let ptr = node_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "128");
    assert_eq!(&[0x00, 0x80], &a.atom(ptr));

    // A leading zero is necessary to encode a positive number with the
    // penultimate byte's most significant bit set
    let num = number_from_u8(&[0x00, 0xff]);
    let ptr = node_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "255");
    assert_eq!(&[0x00, 0xff], &a.atom(ptr));

    let num = number_from_u8(&[0x7f, 0xff]);
    let ptr = node_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "32767");
    assert_eq!(&[0x7f, 0xff], &a.atom(ptr));

    // the first byte is redundant, it's still -1
    let num = number_from_u8(&[0xff, 0xff]);
    let ptr = node_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "-1");
    assert_eq!(&[0xff], &a.atom(ptr));

    let num = number_from_u8(&[0xff]);
    let ptr = node_from_number(&mut a, &num).unwrap();
    assert_eq!(format!("{}", num), "-1");
    assert_eq!(&[0xff], &a.atom(ptr));

    let num = number_from_u8(&[0x00, 0x80, 0x00]);
    assert_eq!(format!("{}", num), "32768");
    let ptr = node_from_number(&mut a, &num).unwrap();
    assert_eq!(&[0x00, 0x80, 0x00], &a.atom(ptr));

    let num = number_from_u8(&[0x00, 0x40, 0x00]);
    assert_eq!(format!("{}", num), "16384");
    let ptr = node_from_number(&mut a, &num).unwrap();
    assert_eq!(&[0x40, 0x00], &a.atom(ptr));
}

#[cfg(test)]
use num_bigint::{BigUint, Sign};

#[cfg(test)]
use std::convert::TryFrom;

#[cfg(test)]
fn roundtrip_bytes(b: &[u8]) {
    let negative = !b.is_empty() && (b[0] & 0x80) != 0;
    let zero = b.is_empty() || (b.len() == 1 && b[0] == 0);

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
        // num-bigin produces a single 0 byte for the value 0. We expect an
        // empty array
        let round_trip = if round_trip == [0] {
            &round_trip[1..]
        } else {
            &round_trip
        };

        assert_eq!(round_trip, b);

        // test to_bytes_le()
        let (sign, mut buf_le) = num.to_bytes_le();

        // there's a special case for empty input buffers, which will result in
        // a single 0 byte here
        if b.is_empty() {
            assert_eq!(buf_le, &[0]);
            buf_le.remove(0);
        }
        assert!(sign == num.sign());

        // the buffer we get from to_bytes_le() is unsigned (since the sign is
        // returned separately). This means it doesn't ever need to prepend a 0
        // byte when the MSB is set. When we're comparing this against the input
        // buffer, we need to add such 0 byte to buf_le to make them compare
        // equal.
        // the 0 prefix has to be added to the end though, since it's little
        // endian
        if !buf_le.is_empty() && (buf_le.last().unwrap() & 0x80) != 0 {
            buf_le.push(0);
        }

        if sign != Sign::Minus {
            assert!(buf_le.iter().eq(b.iter().rev()));
        } else {
            let negated = -num;
            let magnitude = negated.to_signed_bytes_be();
            assert!(buf_le.iter().eq(magnitude.iter().rev()));
        }
    }

    // test parsing unsigned bytes
    {
        let unsigned_num: Number = BigUint::from_bytes_be(b).into();
        assert!(unsigned_num.sign() != Sign::Minus);
        let unsigned_round_trip = unsigned_num.to_signed_bytes_be();
        let unsigned_round_trip = if unsigned_round_trip == [0] {
            &unsigned_round_trip[1..]
        } else {
            &unsigned_round_trip
        };
        if !b.is_empty() && (b[0] & 0x80) != 0 {
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

    let round_trip: u64 = TryFrom::try_from(num).unwrap();
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
    use std::cmp::Ordering;

    let num: Number = v.into();

    match v.cmp(&0) {
        Ordering::Equal => assert!(num.sign() == Sign::NoSign),
        Ordering::Less => assert!(num.sign() == Sign::Minus),
        Ordering::Greater => assert!(num.sign() == Sign::Plus),
    }

    assert!(num.bits() <= 64);

    let round_trip: i64 = TryFrom::try_from(num).unwrap();
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
    Number::from_signed_bytes_be(b).bits()
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
