use crate::allocator::{Allocator, SExp};

#[derive(Clone, Copy)]
pub struct IntAtomBuf {
    start: u32,
    end: u32,
}

enum NodePtr {
    Atom(IntAtomBuf),
    Pair(u32, u32),
}

pub struct IntAllocator {
    // this is effectively a grow-only stack where atoms are allocated. Atoms
    // are immutable, so once they are created, they will stay around until the
    // program completes
    u8_vec: Vec<u8>,
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
        r.u8_vec.reserve(1024 * 1024);
        r.u8_vec.push(1_u8);
        // Preallocated empty list
        r.node_vec
            .push(NodePtr::Atom(IntAtomBuf { start: 0, end: 0 }));
        // Preallocated 1
        r.node_vec
            .push(NodePtr::Atom(IntAtomBuf { start: 0, end: 1 }));
        r
    }
}

impl Allocator for IntAllocator {
    type Ptr = u32;
    type AtomBuf = IntAtomBuf;

    fn new_atom(&mut self, v: &[u8]) -> Self::Ptr {
        let start = self.u8_vec.len() as u32;
        self.u8_vec.extend_from_slice(v);
        let end = self.u8_vec.len() as u32;
        let r = self.node_vec.len() as u32;
        self.node_vec.push(NodePtr::Atom(IntAtomBuf { start, end }));
        r
    }

    fn new_pair(&mut self, first: Self::Ptr, rest: Self::Ptr) -> Self::Ptr {
        let r: u32 = self.node_vec.len() as u32;
        self.node_vec.push(NodePtr::Pair(first, rest));
        r
    }

    fn atom<'a>(&'a self, node: &'a Self::Ptr) -> &'a [u8] {
        match self.node_vec[*node as usize] {
            NodePtr::Atom(IntAtomBuf { start, end }) => &self.u8_vec[start as usize..end as usize],
            _ => panic!("expected atom, got pair"),
        }
    }

    fn buf<'a>(&'a self, node: &'a Self::AtomBuf) -> &'a [u8] {
        &self.u8_vec[node.start as usize..node.end as usize]
    }

    fn sexp(&self, node: &Self::Ptr) -> SExp<Self::Ptr, Self::AtomBuf> {
        match self.node_vec[*node as usize] {
            NodePtr::Atom(atombuf) => SExp::Atom(atombuf),
            NodePtr::Pair(left, right) => SExp::Pair(left, right),
        }
    }

    fn null(&self) -> Self::Ptr {
        0
    }

    fn one(&self) -> Self::Ptr {
        1
    }
}
