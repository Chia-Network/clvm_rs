use std::alloc::Allocator;

#[repr(u8)]
#[derive(PartialEq, Eq, Clone, Debug, Copy, Hash)]
pub enum ChildPos {
    Left = 0,
    Right = 1,
}

/// Builds a path backwards, starting at the target moving backwards. The bytes
/// are laid out in big-endian order, where the left-most byte is index 0. The
/// path is built from left to right, since it's parsed right to left when
/// followed).
#[derive(Clone, Debug, PartialEq)]
pub struct PathBuilder<A: Allocator> {
    // TODO: It might make sense to implement small object optimization here.
    // The vast majority of paths are just a single byte, statically allocate 8
    // would seem reasonable
    store: Vec<u8, A>,
    /// the bit the next write will happen to (counts down)
    bit_pos: u8,
}

impl<A: Allocator> PathBuilder<A> {
    pub fn new(allocator: A) -> Self {
        Self {
            store: Vec::with_capacity_in(16, allocator),
            bit_pos: 7,
        }
    }

    pub fn clear(&mut self) {
        self.bit_pos = 7;
        self.store.clear();
    }

    pub fn push(&mut self, dir: ChildPos) {
        if self.bit_pos == 7 {
            self.store.push(0);
        }

        if dir == ChildPos::Right {
            *self.store.last_mut().unwrap() |= 1 << self.bit_pos;
        }
        if self.bit_pos == 0 {
            self.bit_pos = 7;
        } else {
            self.bit_pos -= 1;
        }
    }

    pub fn done(mut self) -> Vec<u8, A> {
        if self.bit_pos < 7 {
            let right_shift = self.bit_pos + 1;
            let left_shift = 7 - self.bit_pos;
            // we need to shift all bits to the right, to right-align the path
            let mask = 0xff << left_shift;
            for idx in (1..self.store.len()).rev() {
                self.store[idx] >>= right_shift;
                let from_next = self.store[idx - 1] << left_shift;
                self.store[idx] |= from_next & mask;
            }
            self.store[0] >>= right_shift;
        }
        self.store
    }

    pub fn truncate(&mut self, size: u32) {
        let bit_pos = 7 - (size & 7);
        if bit_pos < 7 {
            let new_len = (size / u8::BITS + 1) as usize;
            self.store.truncate(new_len);
            let mask = 0xff << (bit_pos + 1);
            *self.store.last_mut().unwrap() &= mask;
        } else {
            let new_len = (size / u8::BITS) as usize;
            self.store.truncate(new_len);
        }
        self.bit_pos = bit_pos as u8;
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> u32 {
        if self.bit_pos == 7 {
            (self.store.len() as u32) * u8::BITS
        } else {
            (self.store.len() as u32) * u8::BITS - self.bit_pos as u32 - 1
        }
    }

    /// returns the number of bytes this atom would need to serialize If this,
    /// plus 1 (for the 0xfe introduction) is larger or equal to the one we're
    /// deduplicating, we should leave it.
    pub fn serialized_length(&self) -> u32 {
        let len = self.store.len() as u32;
        match len {
            0 => 1,
            // if we have one byte, the top bit determines whether we can
            // serialize it as a single byte or if we need a length prefix
            1 => {
                if self.bit_pos == 7 && self.store[0] >= 80 {
                    2
                } else {
                    1
                }
            }
            2..=0x3f => 1 + len,
            0x40..=0x1ff => 2 + len,
            0x200..=0xfffff => 3 + len,
            0x1000000..=0x7ffffff => 4 + len,
            _ => 5 + len,
        }
    }

    /// returns true if self is better ithan or equal to the right hand side. We
    /// use this to decide whether to replace the best path we've found so far. If
    /// they're equally good, we prefer to not make any changes. The metric we use
    /// is shorter is better and lexicographically smaller is better.
    pub fn better_than(&self, rhs: &Self) -> bool {
        use std::cmp::Ordering;

        let rhs_len = rhs.len();
        let lhs_len = self.len();
        match lhs_len.cmp(&rhs_len) {
            Ordering::Less => true,
            Ordering::Greater => false,
            Ordering::Equal => self.store <= rhs.store,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde::serialized_length_atom;
    use hex;
    use rstest::rstest;
    use std::alloc::System;

    fn build_path(input: &[u8]) -> PathBuilder<System> {
        let mut path = PathBuilder::new(System);
        // keep in mind that paths are built in reverse order (starting from the
        // target).
        for (idx, b) in input.iter().enumerate() {
            assert_eq!(path.len(), idx as u32);
            path.push(if *b == 0 {
                ChildPos::Left
            } else {
                ChildPos::Right
            });
        }
        path
    }

    #[rstest]
    #[case(&[1], "01")]
    #[case(&[1,0], "02")]
    #[case(&[1,0,0], "04")]
    #[case(&[1,0,0,0], "08")]
    #[case(&[1,0,0,0,0], "10")]
    #[case(&[1,0,0,0,0,0], "20")]
    #[case(&[1,0,0,0,0,0,0], "40")]
    #[case(&[1,0,0,0,0,0,0,0], "80")]
    #[case(&[1,0,0,0,0,0,0,0,0], "0100")]
    #[case(&[1,0,0,0,0,0,0,0,0,0], "0200")]
    #[case(&[1,0,0,0,0,0,0,0,0,0,0], "0400")]
    #[case(&[1,0,0,0,0,0,0,0,0,0,0,0], "0800")]
    #[case(&[1,0,0,0,0,0,0,0,0,0,0,0,0], "1000")]
    #[case(&[1,0,0,0,0,0,0,0,0,0,0,0,0,0], "2000")]
    #[case(&[1,0,0,0,0,0,0,0,0,0,0,0,0,0,0], "4000")]
    #[case(&[1,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0], "8000")]
    #[case(&[1,1,1,0,0], "1c")]
    #[case(&[1,0,1,0,0,1,0,0,0], "0148")]
    fn test_build(#[case] input: &[u8], #[case] expect: &str) {
        let path = build_path(input);
        let ret = path.done();
        assert_eq!(hex::encode(ret), expect);
    }

    #[rstest]
    #[case(15, 0, "")]
    #[case(15, 1, "01")]
    #[case(15, 2, "03")]
    #[case(15, 3, "07")]
    #[case(15, 4, "0f")]
    #[case(15, 5, "1f")]
    #[case(15, 6, "3f")]
    #[case(15, 7, "7f")]
    #[case(15, 8, "ff")]
    #[case(15, 9, "01ff")]
    #[case(15, 10, "03ff")]
    #[case(15, 11, "07ff")]
    #[case(15, 12, "0fff")]
    #[case(15, 13, "1fff")]
    #[case(15, 14, "3fff")]
    #[case(15, 15, "7fff")]
    #[case(80, 0, "")]
    #[case(80, 1, "01")]
    #[case(80, 2, "03")]
    #[case(80, 3, "07")]
    #[case(80, 4, "0f")]
    #[case(80, 5, "1f")]
    #[case(80, 6, "3f")]
    #[case(80, 7, "7f")]
    #[case(80, 8, "ff")]
    #[case(80, 9, "01ff")]
    #[case(80, 10, "03ff")]
    #[case(80, 11, "07ff")]
    #[case(80, 12, "0fff")]
    #[case(80, 13, "1fff")]
    #[case(80, 14, "3fff")]
    #[case(80, 15, "7fff")]
    #[case(80, 80, "ffffffffffffffffffff")]
    #[case(80, 79, "7fffffffffffffffffff")]
    fn test_truncate(#[case] num_bits: usize, #[case] truncate: u32, #[case] expect: &str) {
        let mut path = PathBuilder::new(System);
        for _i in 0..num_bits {
            path.push(ChildPos::Right);
        }
        path.truncate(truncate);
        assert_eq!(path.len(), truncate);
        let ret = path.done();
        assert_eq!(hex::encode(ret), expect);
    }

    #[rstest]
    #[case(15, 0, "01")]
    #[case(15, 1, "03")]
    #[case(15, 2, "07")]
    #[case(15, 3, "0f")]
    #[case(15, 4, "1f")]
    #[case(15, 5, "3f")]
    #[case(15, 6, "7f")]
    #[case(15, 7, "ff")]
    #[case(15, 8, "01ff")]
    #[case(15, 9, "03ff")]
    #[case(15, 10, "07ff")]
    #[case(15, 11, "0fff")]
    #[case(15, 12, "1fff")]
    #[case(15, 13, "3fff")]
    #[case(15, 14, "7fff")]
    #[case(15, 15, "ffff")]
    #[case(80, 0, "01")]
    #[case(80, 1, "03")]
    #[case(80, 2, "07")]
    #[case(80, 3, "0f")]
    #[case(80, 4, "1f")]
    #[case(80, 5, "3f")]
    #[case(80, 6, "7f")]
    #[case(80, 7, "ff")]
    #[case(80, 8, "01ff")]
    #[case(80, 9, "03ff")]
    #[case(80, 10, "07ff")]
    #[case(80, 11, "0fff")]
    #[case(80, 12, "1fff")]
    #[case(80, 13, "3fff")]
    #[case(80, 14, "7fff")]
    #[case(80, 15, "ffff")]
    #[case(80, 79, "ffffffffffffffffffff")]
    fn test_truncate_add(#[case] num_bits: usize, #[case] truncate: u32, #[case] expect: &str) {
        let mut path = PathBuilder::new(System);
        for _i in 0..num_bits {
            path.push(ChildPos::Right);
        }
        path.truncate(truncate);
        path.push(ChildPos::Right);
        let ret = path.done();
        assert_eq!(hex::encode(ret), expect);
    }

    #[rstest]
    fn test_clear(
        #[values(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17)] num_bits: usize,
    ) {
        let mut path = PathBuilder::new(System);
        for _i in 0..num_bits {
            path.push(ChildPos::Right);
        }
        path.clear();
        assert!(path.done().is_empty());
    }

    #[rstest]
    // length
    #[case(&[1], &[1], true)]
    #[case(&[1], &[1,1], true)]
    #[case(&[1, 1], &[1], false)]
    #[case(&[1], &[1], true)]
    // byte boundary 7 & 8 bits
    #[case(&[1,1,1,1,1,1,1],   &[1,1,1,1,1,1,1], true)]
    #[case(&[1,1,1,1,1,1,1],   &[1,1,1,1,1,1,1,1], true)]
    #[case(&[1,1,1,1,1,1,1,1], &[1,1,1,1,1,1,1], false)]
    // byte boundary 8 & 9 bits
    #[case(&[1,1,1,1,1,1,1,1],   &[1,1,1,1,1,1,1,1], true)]
    #[case(&[1,1,1,1,1,1,1,1,1], &[1,1,1,1,1,1,1,1,1], true)]
    #[case(&[1,1,1,1,1,1,1,1],   &[1,1,1,1,1,1,1,1,1], true)]
    #[case(&[1,1,1,1,1,1,1,1,1], &[1,1,1,1,1,1,1,1], false)]
    // lexicographic
    #[case(&[1,0], &[1,1], true)]
    #[case(&[1,1], &[1,1], true)]
    #[case(&[1,1], &[1,0], false)]
    // byte boundary
    #[case(&[1,0,0,0,0,0,0,0,0], &[1,1,0,0,0,0,0,0,0], true)]
    #[case(&[1,0,0,0,0,0,0,0,1], &[1,1,0,0,0,0,0,0,0], true)]
    #[case(&[1,1,0,0,0,0,0,0,0], &[1,1,0,0,0,0,0,0,0], true)]
    #[case(&[1,1,0,0,0,0,0,0,1], &[1,1,0,0,0,0,0,0,0], false)]
    #[case(&[1,1,0,0,0,0,0,0,0], &[1,0,0,0,0,0,0,0,0], false)]
    #[case(&[1,1,0,0,0,0,0,0,1], &[1,0,0,0,0,0,0,0,0], false)]
    fn test_better_than(#[case] lhs: &[u8], #[case] rhs: &[u8], #[case] expect: bool) {
        let lhs = build_path(lhs);
        let rhs = build_path(rhs);
        assert_eq!(lhs.better_than(&rhs), expect);
    }

    #[rstest]
    #[case(0)]
    #[case(1)]
    #[case(6)]
    #[case(7)]
    #[case(8)]
    #[case(9)]
    #[case(31)]
    #[case(32)]
    #[case(33)]
    #[case(504)]
    #[case(505)]
    #[case(511)]
    #[case(512)]
    #[case(513)]
    #[case(0xfff9)]
    fn test_serialized_length(#[case] num_bits: u32) {
        let mut path = PathBuilder::new(System);
        for _ in 0..num_bits {
            path.push(ChildPos::Right);
        }
        let ser_len = path.serialized_length();
        let vec = path.done();
        assert_eq!(serialized_length_atom(&vec), ser_len);
    }
}
