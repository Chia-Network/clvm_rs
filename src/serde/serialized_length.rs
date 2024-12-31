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
}
