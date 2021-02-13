use crate::allocator::{Allocator, SExp};

#[derive(Clone, Copy)]
pub struct IntAtomBuf {
    start: u32,
    end: u32,
}

#[derive(Clone, Copy)]
pub struct IntPair {
    first: i32,
    rest: i32,
}

pub struct IntAllocator {
    // this is effectively a grow-only stack where atoms are allocated. Atoms
    // are immutable, so once they are created, they will stay around until the
    // program completes
    u8_vec: Vec<u8>,

    // storage for all pairs (positive indices)
    pair_vec: Vec<IntPair>,

    // storage for all atoms (negative indices).
    // node index -1 refers to index 0 in this vector, -2 refers to 1 and so
    // on.
    atom_vec: Vec<IntAtomBuf>,
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
            pair_vec: Vec::new(),
            atom_vec: Vec::new(),
        };
        r.u8_vec.reserve(1024 * 1024);
        r.atom_vec.reserve(256);
        r.pair_vec.reserve(256);
        r.u8_vec.push(1_u8);
        // Preallocated empty list
        r.atom_vec.push(IntAtomBuf { start: 0, end: 0 });
        // Preallocated 1
        r.atom_vec.push(IntAtomBuf { start: 0, end: 1 });
        r
    }
}

impl Allocator for IntAllocator {
    type Ptr = i32;
    type AtomBuf = IntAtomBuf;

    fn new_atom(&mut self, v: &[u8]) -> Self::Ptr {
        let start = self.u8_vec.len() as u32;
        self.u8_vec.extend_from_slice(v);
        let end = self.u8_vec.len() as u32;
        self.atom_vec.push(IntAtomBuf { start, end });
        -(self.atom_vec.len() as i32)
    }

    fn new_pair(&mut self, first: Self::Ptr, rest: Self::Ptr) -> Self::Ptr {
        let r = self.pair_vec.len() as i32;
        self.pair_vec.push(IntPair { first, rest });
        r
    }

    fn atom<'a>(&'a self, node: &'a Self::Ptr) -> &'a [u8] {
        if *node >= 0 {
            panic!("expected atom, got pair");
        }
        let atom = self.atom_vec[(-*node - 1) as usize];
        &self.u8_vec[atom.start as usize..atom.end as usize]
    }

    fn buf<'a>(&'a self, node: &'a Self::AtomBuf) -> &'a [u8] {
        &self.u8_vec[node.start as usize..node.end as usize]
    }

    fn sexp(&self, node: &Self::Ptr) -> SExp<Self::Ptr, Self::AtomBuf> {
        if *node >= 0 {
            let pair = self.pair_vec[*node as usize];
            SExp::Pair(pair.first, pair.rest)
        } else {
            let atom = self.atom_vec[(-*node - 1) as usize];
            SExp::Atom(atom)
        }
    }

    fn null(&self) -> Self::Ptr {
        -1
    }

    fn one(&self) -> Self::Ptr {
        -2
    }
}
