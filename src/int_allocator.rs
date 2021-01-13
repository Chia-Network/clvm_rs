use std::vec;

use aovec::Aovec;

use crate::allocator::{Allocator, SExp};
use crate::reduction::EvalErr;

enum NodePtr {
    Atom(u32),
    Pair(u32, u32),
}

pub struct IntAllocator {
    u8_vec: Aovec<Vec<u8>>,
    node_vec: Aovec<NodePtr>,
}

impl Default for IntAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl IntAllocator {
    pub fn new() -> Self {
        let r = IntAllocator {
            u8_vec: Aovec::new(1024 * 1024),
            node_vec: Aovec::new(32768),
        };
        r.u8_vec.push(vec![]);
        r.u8_vec.push(vec![1_u8]);
        r.node_vec.push(NodePtr::Atom(0));
        r.node_vec.push(NodePtr::Atom(1));
        r
    }
}

impl Allocator<u32> for IntAllocator {
    fn new_atom(&self, v: &[u8]) -> u32 {
        let index = self.u8_vec.len() as u32;
        self.u8_vec.push(v.into());
        let r: u32 = self.node_vec.len() as u32;
        self.node_vec.push(NodePtr::Atom(index));
        r
    }

    fn new_pair(&self, first: &u32, rest: &u32) -> u32 {
        let r: u32 = self.node_vec.len() as u32;
        self.node_vec.push(NodePtr::Pair(*first, *rest));
        r
    }

    fn sexp(&self, node: &u32) -> SExp<u32> {
        match self.node_vec[*node as usize] {
            NodePtr::Atom(index) => {
                let atom = &self.u8_vec[index as usize];
                SExp::Atom(&atom)
            }
            NodePtr::Pair(left, right) => SExp::Pair(left, right),
        }
    }

    fn make_clone(&self, node: &u32) -> u32 {
        *node
    }

    fn null(&self) -> u32 {
        0
    }

    fn one(&self) -> u32 {
        1
    }
}

impl IntAllocator {
    pub fn err<T>(&self, node: &u32, msg: &str) -> Result<T, EvalErr<u32>> {
        Err(EvalErr(self.make_clone(node), msg.into()))
    }
}
