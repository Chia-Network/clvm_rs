use rug::integer::Order;
use rug::Integer;

pub type Number = Integer;

/// Convert big-endian signed (two's complement) bytes to Number.
pub fn number_from_u8(v: &[u8]) -> Number {
    if v.is_empty() {
        return Number::new();
    }
    let n = Number::from_digits(v, Order::MsfBe);
    if (v[0] & 0x80) != 0 {
        n - (Number::from(1) << (8 * v.len()))
    } else {
        n
    }
}

/// Serialize Number to minimal big-endian signed (two's complement) bytes.
pub fn number_to_signed_bytes_be(n: &Number) -> Vec<u8> {
    if *n == 0 {
        return vec![];
    }
    if *n > 0 {
        let mut d = n.to_digits::<u8>(Order::MsfBe);
        if !d.is_empty() && (d[0] & 0x80) != 0 {
            d.insert(0, 0);
        }
        d
    } else {
        let bits = n.signed_bits();
        let n_bytes = ((bits as usize) + 8) / 8;
        let mod_val = Number::from(1) << (8 * n_bytes);
        let complement = mod_val + n.clone();
        let mut d = complement.to_digits::<u8>(Order::MsfBe);
        while d.len() > 1 && d[0] == 0xff && (d[1] & 0x80) != 0 {
            d.remove(0);
        }
        d
    }
}

#[cfg(test)]
mod tests {
    use rug::integer::Order;
    use rug::Integer;

    use super::*;

    fn roundtrip_bytes(b: &[u8]) {
        let negative = !b.is_empty() && (b[0] & 0x80) != 0;
        let zero = b.is_empty() || (b.len() == 1 && b[0] == 0);

        {
            let num = number_from_u8(b);

            if negative {
                assert!(num < 0);
            } else if zero {
                assert!(num == 0);
            } else {
                assert!(num > 0);
            }

            let round_trip = number_to_signed_bytes_be(&num);
            let round_trip = if round_trip == [0] {
                &round_trip[1..]
            } else {
                &round_trip[..]
            };

            assert_eq!(round_trip, b);

            // test to_digits LE (magnitude)
            let (sign_neg, mut buf_le): (bool, Vec<u8>) = if num == 0 {
                (false, vec![0])
            } else if num < 0 {
                let abs = (-num.clone()).to_digits::<u8>(Order::Lsf);
                (true, abs)
            } else {
                (false, num.to_digits::<u8>(Order::Lsf))
            };
            if b.is_empty() {
                assert_eq!(buf_le, &[0]);
                buf_le.clear();
            }
            assert_eq!(sign_neg, num < 0);

            if !buf_le.is_empty() && (buf_le.last().unwrap() & 0x80) != 0 {
                buf_le.push(0);
            }

            if !sign_neg {
                assert!(buf_le.iter().eq(b.iter().rev()));
            } else {
                let negated = -num;
                let magnitude = number_to_signed_bytes_be(&negated);
                assert!(buf_le.iter().eq(magnitude.iter().rev()));
            }
        }

        // test parsing unsigned bytes
        {
            let unsigned_num: Number = Integer::from_digits(b, Order::MsfBe);
            assert!(unsigned_num >= 0);
            let unsigned_round_trip = number_to_signed_bytes_be(&unsigned_num);
            let unsigned_round_trip = if unsigned_round_trip == [0] {
                &unsigned_round_trip[1..]
            } else {
                &unsigned_round_trip[..]
            };
            if !b.is_empty() && (b[0] & 0x80) != 0 {
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

    fn roundtrip_u64(v: u64) {
        let num: Number = v.into();
        assert!(num >= 0);

        assert!(num.significant_bits() <= 64);

        let round_trip: u64 = num.to_u64().unwrap();
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

    fn roundtrip_i64(v: i64) {
        use std::cmp::Ordering;

        let num: Number = v.into();

        match v.cmp(&0) {
            Ordering::Equal => assert!(num == 0),
            Ordering::Less => assert!(num < 0),
            Ordering::Greater => assert!(num > 0),
        }

        assert!(num.significant_bits() <= 64);

        let round_trip: i64 = num.to_i64().unwrap();
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

    fn bits(b: &[u8]) -> u32 {
        number_from_u8(b).significant_bits()
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
}
