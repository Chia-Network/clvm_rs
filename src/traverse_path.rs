use crate::allocator::{Allocator, NodePtr, SExp};
use crate::cost::Cost;
use crate::error::EvalErr;
use crate::reduction::{Reduction, Response};

// lowered from measured 147 per bit. It doesn't seem to take this long in
// practice
const TRAVERSE_BASE_COST: Cost = 40;
const TRAVERSE_COST_PER_ZERO_BYTE: Cost = 4;
const TRAVERSE_COST_PER_BIT: Cost = 4;

// `run_program` has two stacks: the operand stack (of `Node` objects) and the
// operator stack (of Operation)

// return a bitmask with a single bit set, for the most significant set bit in
// the input byte
pub(crate) fn msb_mask(byte: u8) -> u8 {
    let mut byte = (byte | (byte >> 1)) as u32;
    byte |= byte >> 2;
    byte |= byte >> 4;
    debug_assert!((byte + 1) >> 1 <= 0x80);
    ((byte + 1) >> 1) as u8
}

// return the index of the first non-zero byte in buf. If all bytes are 0, the
// length (one past end) will be returned.
pub const fn first_non_zero(buf: &[u8]) -> usize {
    let mut c: usize = 0;
    while c < buf.len() && buf[c] == 0 {
        c += 1;
    }
    c
}

pub fn traverse_path(allocator: &Allocator, node_index: &[u8], args: NodePtr) -> Response {
    let mut arg_list: NodePtr = args;

    // find first non-zero byte
    let first_bit_byte_index = first_non_zero(node_index);

    let mut cost: Cost = TRAVERSE_BASE_COST
        + (first_bit_byte_index as Cost) * TRAVERSE_COST_PER_ZERO_BYTE
        + TRAVERSE_COST_PER_BIT;

    if first_bit_byte_index >= node_index.len() {
        return Ok(Reduction(cost, allocator.nil()));
    }

    // find first non-zero bit (the most significant bit is a sentinel)
    let last_bitmask = msb_mask(node_index[first_bit_byte_index]);

    // follow through the bits, moving left and right
    let mut byte_idx = node_index.len() - 1;
    let mut bitmask = 0x01;
    while byte_idx > first_bit_byte_index || bitmask < last_bitmask {
        let is_bit_set: bool = (node_index[byte_idx] & bitmask) != 0;
        match allocator.sexp(arg_list) {
            SExp::Atom => {
                return Err(EvalErr::PathIntoAtom);
            }
            SExp::Pair(left, right) => {
                arg_list = if is_bit_set { right } else { left };
            }
        }
        if bitmask == 0x80 {
            bitmask = 0x01;
            byte_idx -= 1;
        } else {
            bitmask <<= 1;
        }
        cost += TRAVERSE_COST_PER_BIT;
    }
    Ok(Reduction(cost, arg_list))
}

// The cost calculation for this version of traverse_path assumes the node_index has the canonical
// integer representation (which is true for SmallAtom in the allocator). If there are any
// redundant leading zeros, the slow path must be used
pub fn traverse_path_fast(allocator: &Allocator, mut node_index: u32, args: NodePtr) -> Response {
    if node_index == 0 {
        return Ok(Reduction(
            TRAVERSE_BASE_COST + TRAVERSE_COST_PER_BIT,
            allocator.nil(),
        ));
    }

    let mut arg_list: NodePtr = args;

    let mut cost: Cost = TRAVERSE_BASE_COST + TRAVERSE_COST_PER_BIT;
    let mut num_bits = 0;
    while node_index != 1 {
        let SExp::Pair(left, right) = allocator.sexp(arg_list) else {
            return Err(EvalErr::PathIntoAtom);
        };

        let is_bit_set: bool = (node_index & 0x01) != 0;
        arg_list = if is_bit_set { right } else { left };
        node_index >>= 1;
        num_bits += 1
    }

    cost += num_bits * TRAVERSE_COST_PER_BIT;
    // since positive numbers sometimes need a leading zero, e.g. 0x80, 0x8000 etc. We also
    // need to add the cost of that leading zero byte
    if num_bits == 7 || num_bits == 15 || num_bits == 23 || num_bits == 31 {
        cost += TRAVERSE_COST_PER_ZERO_BYTE;
    }

    Ok(Reduction(cost, arg_list))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msb_mask() {
        assert_eq!(msb_mask(0x0), 0x0);
        assert_eq!(msb_mask(0x01), 0x01);
        assert_eq!(msb_mask(0x02), 0x02);
        assert_eq!(msb_mask(0x04), 0x04);
        assert_eq!(msb_mask(0x08), 0x08);
        assert_eq!(msb_mask(0x10), 0x10);
        assert_eq!(msb_mask(0x20), 0x20);
        assert_eq!(msb_mask(0x40), 0x40);
        assert_eq!(msb_mask(0x80), 0x80);

        assert_eq!(msb_mask(0x44), 0x40);
        assert_eq!(msb_mask(0x2a), 0x20);
        assert_eq!(msb_mask(0xff), 0x80);
        assert_eq!(msb_mask(0x0f), 0x08);
    }

    #[test]
    fn test_first_non_zero() {
        assert_eq!(first_non_zero(&[]), 0);
        assert_eq!(first_non_zero(&[1]), 0);
        assert_eq!(first_non_zero(&[0]), 1);
        assert_eq!(first_non_zero(&[0, 0, 0, 1, 1, 1]), 3);
        assert_eq!(first_non_zero(&[0, 0, 0, 0, 0, 0]), 6);
        assert_eq!(first_non_zero(&[1, 0, 0, 0, 0, 0]), 0);
    }

    #[test]
    fn test_traverse_path() {
        use crate::allocator::Allocator;

        let mut a = Allocator::new();
        let nul = a.nil();
        let n1 = a.new_atom(&[0, 1, 2]).unwrap();
        let n2 = a.new_atom(&[4, 5, 6]).unwrap();

        assert_eq!(traverse_path(&a, &[], n1).unwrap(), Reduction(44, nul));
        assert_eq!(traverse_path(&a, &[0b1], n1).unwrap(), Reduction(44, n1));
        assert_eq!(traverse_path(&a, &[0b1], n2).unwrap(), Reduction(44, n2));

        // cost for leading zeros
        assert_eq!(traverse_path(&a, &[0], n1).unwrap(), Reduction(48, nul));
        assert_eq!(traverse_path(&a, &[0, 0], n1).unwrap(), Reduction(52, nul));
        assert_eq!(
            traverse_path(&a, &[0, 0, 0], n1).unwrap(),
            Reduction(56, nul)
        );
        assert_eq!(
            traverse_path(&a, &[0, 0, 0, 0], n1).unwrap(),
            Reduction(60, nul)
        );

        let n3 = a.new_pair(n1, n2).unwrap();
        assert_eq!(traverse_path(&a, &[0b1], n3).unwrap(), Reduction(44, n3));
        assert_eq!(traverse_path(&a, &[0b10], n3).unwrap(), Reduction(48, n1));
        assert_eq!(traverse_path(&a, &[0b11], n3).unwrap(), Reduction(48, n2));
        assert_eq!(traverse_path(&a, &[0b11], n3).unwrap(), Reduction(48, n2));

        let list = a.new_pair(n1, nul).unwrap();
        let list = a.new_pair(n2, list).unwrap();

        assert_eq!(traverse_path(&a, &[0b10], list).unwrap(), Reduction(48, n2));
        assert_eq!(
            traverse_path(&a, &[0b101], list).unwrap(),
            Reduction(52, n1)
        );
        assert_eq!(
            traverse_path(&a, &[0b111], list).unwrap(),
            Reduction(52, nul)
        );

        // errors
        assert_eq!(
            traverse_path(&a, &[0b1011], list).unwrap_err(),
            EvalErr::PathIntoAtom
        );
        assert_eq!(
            traverse_path(&a, &[0b1101], list).unwrap_err(),
            EvalErr::PathIntoAtom
        );
        assert_eq!(
            traverse_path(&a, &[0b1001], list).unwrap_err(),
            EvalErr::PathIntoAtom
        );
        assert_eq!(
            traverse_path(&a, &[0b1010], list).unwrap_err(),
            EvalErr::PathIntoAtom
        );
        assert_eq!(
            traverse_path(&a, &[0b1110], list).unwrap_err(),
            EvalErr::PathIntoAtom
        );
    }

    #[test]
    fn test_traverse_path_fast_fast() {
        use crate::allocator::Allocator;

        let mut a = Allocator::new();
        let nul = a.nil();
        let n1 = a.new_atom(&[0, 1, 2]).unwrap();
        let n2 = a.new_atom(&[4, 5, 6]).unwrap();

        assert_eq!(traverse_path_fast(&a, 0, n1).unwrap(), Reduction(44, nul));
        assert_eq!(traverse_path_fast(&a, 0b1, n1).unwrap(), Reduction(44, n1));
        assert_eq!(traverse_path_fast(&a, 0b1, n2).unwrap(), Reduction(44, n2));

        let n3 = a.new_pair(n1, n2).unwrap();
        assert_eq!(traverse_path_fast(&a, 0b1, n3).unwrap(), Reduction(44, n3));
        assert_eq!(traverse_path_fast(&a, 0b10, n3).unwrap(), Reduction(48, n1));
        assert_eq!(traverse_path_fast(&a, 0b11, n3).unwrap(), Reduction(48, n2));
        assert_eq!(traverse_path_fast(&a, 0b11, n3).unwrap(), Reduction(48, n2));

        let list = a.new_pair(n1, nul).unwrap();
        let list = a.new_pair(n2, list).unwrap();

        assert_eq!(
            traverse_path_fast(&a, 0b10, list).unwrap(),
            Reduction(48, n2)
        );
        assert_eq!(
            traverse_path_fast(&a, 0b101, list).unwrap(),
            Reduction(52, n1)
        );
        assert_eq!(
            traverse_path_fast(&a, 0b111, list).unwrap(),
            Reduction(52, nul)
        );

        // errors
        assert_eq!(
            traverse_path_fast(&a, 0b1011, list).unwrap_err(),
            EvalErr::PathIntoAtom
        );
        assert_eq!(
            traverse_path_fast(&a, 0b1101, list).unwrap_err(),
            EvalErr::PathIntoAtom
        );
        assert_eq!(
            traverse_path_fast(&a, 0b1001, list).unwrap_err(),
            EvalErr::PathIntoAtom
        );
        assert_eq!(
            traverse_path_fast(&a, 0b1010, list).unwrap_err(),
            EvalErr::PathIntoAtom
        );
        assert_eq!(
            traverse_path_fast(&a, 0b1110, list).unwrap_err(),
            EvalErr::PathIntoAtom
        );
    }
}
