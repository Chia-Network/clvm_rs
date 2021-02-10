use std::vec;

use crate::allocator::{Allocator, SExp};

enum NodePtr {
    Atom(u32),
    Pair(u32, u32),
}

pub struct IntAllocator {
    u8_vec: Vec<Vec<u8>>,
    node_vec: Vec<NodePtr>,
}

impl Default for IntAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl IntAllocator {
    pub fn new() -> Self {
        let mut r = IntAllocator {
            u8_vec: Vec::new(),
            node_vec: Vec::new(),
        };
        r.u8_vec.push(vec![]);
        r.u8_vec.push(vec![1_u8]);
        r.node_vec.push(NodePtr::Atom(0));
        r.node_vec.push(NodePtr::Atom(1));
        r
    }
}

impl Allocator for IntAllocator {
    type Ptr = u32;
    type AtomBuf = u32;

    fn new_atom(&mut self, v: &[u8]) -> u32 {
        let index = self.u8_vec.len() as u32;
        self.u8_vec.push(v.into());
        let r: u32 = self.node_vec.len() as u32;
        self.node_vec.push(NodePtr::Atom(index));
        r
    }

    fn new_pair(&mut self, first: Self::Ptr, rest: Self::Ptr) -> Self::Ptr {
        let r: u32 = self.node_vec.len() as u32;
        self.node_vec.push(NodePtr::Pair(first, rest));
        r
    }

    fn atom<'a>(&'a self, node: &'a Self::Ptr) -> &'a [u8] {
        match self.node_vec[*node as usize] {
            NodePtr::Atom(index) => &self.u8_vec[index as usize],
            _ => panic!("expected atom, got pair"),
        }
    }

    fn buf<'a>(&'a self, node: &'a Self::AtomBuf) -> &'a [u8] {
        &self.u8_vec[*node as usize]
    }

    fn sexp(&self, node: &Self::Ptr) -> SExp<Self::Ptr, Self::AtomBuf> {
        match self.node_vec[*node as usize] {
            NodePtr::Atom(index) => SExp::Atom(index),
            NodePtr::Pair(left, right) => SExp::Pair(left, right),
        }
    }

    fn null(&self) -> u32 {
        0
    }

    fn one(&self) -> u32 {
        1
    }
}
