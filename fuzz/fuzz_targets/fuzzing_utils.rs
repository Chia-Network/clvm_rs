use chia_sha2::Sha256;
use clvmr::allocator::{Allocator, NodePtr, SExp};

pub struct BitCursor<'a> {
    data: &'a [u8],
    bit_offset: u8,
}

fn mask(num: u8) -> u8 {
    0xff >> num
}

impl<'a> BitCursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        BitCursor {
            data,
            bit_offset: 0,
        }
    }

    pub fn read_bits(&mut self, mut num: u8) -> Option<u8> {
        assert!(num <= 8);
        let ret = if self.data.is_empty() {
            num = 0;
            None
        } else if self.bit_offset + num <= 8 {
            Some((self.data[0] & mask(self.bit_offset)) >> (8 - num - self.bit_offset))
        } else if self.data.len() < 2 {
            num = 8 - self.bit_offset;
            Some(self.data[0] & mask(self.bit_offset))
        } else {
            let first_byte = 8 - self.bit_offset;
            let second_byte = num - first_byte;
            Some(
                ((self.data[0] & mask(self.bit_offset)) << second_byte)
                    | (self.data[1] >> (8 - second_byte)),
            )
        };
        self.advance(num);
        ret
    }

    fn advance(&mut self, bits: u8) {
        let bits = self.bit_offset as u32 + bits as u32;
        if bits >= 8 {
            self.data = &self.data[(bits / 8) as usize..];
        }
        self.bit_offset = (bits % 8) as u8;
    }
}

const BUFFER: [u8; 63] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

pub fn make_tree(a: &mut Allocator, cursor: &mut BitCursor, short_atoms: bool) -> NodePtr {
    match cursor.read_bits(1) {
        None => a.nil(),
        Some(0) => {
            let first = make_tree(a, cursor, short_atoms);
            let second = make_tree(a, cursor, short_atoms);
            a.new_pair(first, second).unwrap()
        }
        Some(_) => {
            if short_atoms {
                match cursor.read_bits(8) {
                    None => a.nil(),
                    Some(val) => a.new_atom(&[val]).unwrap(),
                }
            } else {
                match cursor.read_bits(6) {
                    None => a.nil(),
                    Some(len) => a.new_atom(&BUFFER[..len as usize]).unwrap(),
                }
            }
        }
    }
}

#[allow(dead_code)]
fn hash_atom(buf: &[u8]) -> [u8; 32] {
    let mut ctx = Sha256::new();
    ctx.update([1_u8]);
    ctx.update(buf);
    ctx.finalize()
}

#[allow(dead_code)]
fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut ctx = Sha256::new();
    ctx.update([2_u8]);
    ctx.update(left);
    ctx.update(right);
    ctx.finalize()
}

#[allow(dead_code)]
enum TreeOp {
    SExp(NodePtr),
    Cons,
}

#[allow(dead_code)]
pub fn tree_hash(a: &Allocator, node: NodePtr) -> [u8; 32] {
    let mut hashes = Vec::new();
    let mut ops = vec![TreeOp::SExp(node)];

    while let Some(op) = ops.pop() {
        match op {
            TreeOp::SExp(node) => match a.sexp(node) {
                SExp::Atom => {
                    hashes.push(hash_atom(a.atom(node).as_ref()));
                }
                SExp::Pair(left, right) => {
                    ops.push(TreeOp::Cons);
                    ops.push(TreeOp::SExp(left));
                    ops.push(TreeOp::SExp(right));
                }
            },
            TreeOp::Cons => {
                let first = hashes.pop().unwrap();
                let rest = hashes.pop().unwrap();
                hashes.push(hash_pair(&first, &rest));
            }
        }
    }

    assert!(hashes.len() == 1);
    hashes[0]
}

#[allow(dead_code)]
pub fn visit_tree(a: &Allocator, node: NodePtr, mut visit: impl FnMut(&Allocator, NodePtr)) {
    let mut nodes = vec![node];
    let mut visited_index = 0;

    while nodes.len() > visited_index {
        match a.sexp(nodes[visited_index]) {
            SExp::Atom => {}
            SExp::Pair(left, right) => {
                nodes.push(left);
                nodes.push(right);
            }
        }
        visited_index += 1;
    }

    // visit nodes bottom-up (right to left).
    for node in nodes.into_iter().rev() {
        visit(a, node);
    }
}
