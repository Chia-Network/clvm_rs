use bumpalo::Bump;

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
#[derive(Debug, PartialEq)]
pub struct PathBuilder<'a> {
    store: &'a mut [u8],
    in_use: u32,
    /// the bit the next write will happen to (counts down)
    bit_pos: u8,
}

impl Default for PathBuilder<'_> {
    fn default() -> Self {
        Self {
            store: &mut [],
            in_use: 0,
            bit_pos: 7,
        }
    }
}

impl<'a> PathBuilder<'a> {
    pub fn push(&mut self, a: &'a Bump, dir: ChildPos) {
        if self.bit_pos == 7 {
            if self.in_use as usize == self.store.len() {
                let old_size = self.store.len();
                let new_size = std::cmp::max(old_size * 2, 16);
                let new_store = a.alloc_slice_fill_default::<u8>(new_size);
                new_store[0..old_size].copy_from_slice(self.store);
                self.store = new_store;
            }
            self.in_use += 1;
        }

        assert!(self.in_use > 0);
        assert!(self.store.len() >= self.in_use as usize);

        if dir == ChildPos::Right {
            self.store[self.in_use as usize - 1] |= 1 << self.bit_pos;
        }
        if self.bit_pos == 0 {
            self.bit_pos = 7;
        } else {
            self.bit_pos -= 1;
        }
    }

    pub fn clone(&self, a: &'a Bump) -> Self {
        Self {
            store: a.alloc_slice_copy(self.store),
            in_use: self.in_use,
            bit_pos: self.bit_pos,
        }
    }

    pub fn done(self) -> Vec<u8> {
        if self.bit_pos < 7 {
            let right_shift = self.bit_pos + 1;
            let left_shift = 7 - self.bit_pos;
            // we need to shift all bits to the right, to right-align the path
            let mask = 0xff << left_shift;
            for idx in (1..self.in_use as usize).rev() {
                self.store[idx] >>= right_shift;
                let from_next = self.store[idx - 1] << left_shift;
                self.store[idx] |= from_next & mask;
            }
            self.store[0] >>= right_shift;
        }
        self.store[0..self.in_use as usize].to_vec()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn len(&self) -> u32 {
        if self.bit_pos == 7 {
            self.in_use * u8::BITS
        } else {
            self.in_use * u8::BITS - self.bit_pos as u32 - 1
        }
    }

    /// returns the number of bytes this atom would need to serialize. If this,
    /// plus 1 (for the 0xfe introduction) is larger or equal to the one we're
    /// deduplicating, we should leave it.
    pub fn serialized_length(&self) -> u32 {
        let len = self.in_use;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde::serialized_length_atom;
    use hex;
    use rstest::rstest;

    fn build_path<'a>(a: &'a Bump, input: &[u8]) -> PathBuilder<'a> {
        let mut path = PathBuilder::default();
        // keep in mind that paths are built in reverse order (starting from the
        // target).
        for (idx, b) in input.iter().enumerate() {
            assert_eq!(path.len(), idx as u32);
            path.push(
                a,
                if *b == 0 {
                    ChildPos::Left
                } else {
                    ChildPos::Right
                },
            );
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
        let a = Bump::new();
        let path = build_path(&a, input);
        let ret = path.done();
        assert_eq!(hex::encode(ret), expect);
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
        let a = Bump::new();
        let mut path = PathBuilder::default();
        for _ in 0..num_bits {
            path.push(&a, ChildPos::Right);
        }
        let ser_len = path.serialized_length();
        let vec = path.done();
        assert_eq!(serialized_length_atom(&vec), ser_len);
    }
}
