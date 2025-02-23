use crate::allocator::len_for_value;

pub fn serialized_length_atom(buf: &[u8]) -> u32 {
    let lb = buf.len() as u32;
    if lb == 0 || (lb == 1 && buf[0] < 128) {
        1
    } else if lb < 0x40 {
        1 + lb
    } else if lb < 0x2000 {
        2 + lb
    } else if lb < 0x100000 {
        3 + lb
    } else if lb < 0x8000000 {
        4 + lb
    } else {
        5 + lb
    }
}

pub fn serialized_length_small_number(val: u32) -> u32 {
    len_for_value(val) as u32 + 1
}

// given an atom with num_bits (counting from the most significant set bit)
// return the number of bytes we need to serialized this atom
pub fn atom_length_bits(num_bits: u64) -> Option<u64> {
    if num_bits < 8 {
        return Some(1);
    }
    let num_bytes = num_bits.div_ceil(8);
    match num_bytes {
        1..0x40 => Some(1 + num_bytes),
        0x40..0x2000 => Some(2 + num_bytes),
        0x2000..0x10_0000 => Some(3 + num_bytes),
        0x10_0000..0x800_0000 => Some(4 + num_bytes),
        0x800_0000..0x4_0000_0000 => Some(5 + num_bytes),
        _ => {
            assert!(num_bits >= 0x4_0000_0000 * 8 - 7);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[rstest]
    #[case(&[], 1)]
    #[case(&[1], 1)]
    #[case(&[0x7f], 1)]
    #[case(&[0x80], 2)]
    #[case(&[0x81], 2)]
    #[case(&[0x80, 0], 3)]
    #[case(&[1; 0x3f], 0x40)]
    #[case(&[1; 0x40], 0x42)]
    fn test_serialized_length_atom(#[case] atom: &[u8], #[case] expect: u32) {
        assert_eq!(serialized_length_atom(atom), expect);
    }

    #[rstest]
    #[case(0, 1)]
    #[case(1, 2)]
    #[case(0x7f, 2)]
    #[case(0x80, 3)]
    #[case(0x7fff, 3)]
    #[case(0x7fffff, 4)]
    #[case(0x800000, 5)]
    #[case(0x7fffffff, 5)]
    #[case(0x80000000, 6)]
    #[case(0xffffffff, 6)]
    fn test_serialized_length_small_number(#[case] value: u32, #[case] expect: u32) {
        assert_eq!(serialized_length_small_number(value), expect);
    }

    #[rstest]
    #[case(0, Some(1))]
    #[case(1, Some(1))]
    #[case(7, Some(1))]
    #[case(8, Some(2))]
    #[case(9, Some(3))]
    #[case(504, Some(1+63))]
    #[case(505, Some(2+64))]
    #[case(0xfff8, Some(2+0x1fff))]
    #[case(0xfff9, Some(3+0x2000))]
    #[case(0x3ffffff8, Some(4 + 0x3ffffff8_u64.div_ceil(8)))]
    #[case(0x3ffffff9, Some(5 + 0x3ffffff9_u64.div_ceil(8)))]
    #[case(0x1ffffffff8, Some(5 + 0x1ffffffff8_u64.div_ceil(8)))]
    #[case(0x1ffffffff9, None)]
    fn test_atom_length_bits(#[case] num_bits: u64, #[case] expect: Option<u64>) {
        assert_eq!(atom_length_bits(num_bits), expect);
    }
}
