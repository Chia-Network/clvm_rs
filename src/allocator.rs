use crate::err_utils::err;
use crate::reduction::EvalErr;

pub type NodePtr = i32;

pub enum SExp {
    Atom(AtomBuf),
    Pair(NodePtr, NodePtr),
}

#[derive(Clone, Copy, Debug)]
pub struct AtomBuf {
    start: u32,
    end: u32,
}

impl AtomBuf {
    pub fn idx_range(&self) -> (u32, u32) {
        (self.start, self.end)
    }
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
    pub fn len(&self) -> usize {
        (self.end - self.start) as usize
    }
}

#[derive(Clone, Copy, Debug)]
pub struct IntPair {
    first: NodePtr,
    rest: NodePtr,
}

#[derive(Debug)]
pub struct Allocator {
    // this is effectively a grow-only stack where atoms are allocated. Atoms
    // are immutable, so once they are created, they will stay around until the
    // program completes
    u8_vec: Vec<u8>,

    // storage for all pairs (positive indices)
    pair_vec: Vec<IntPair>,

    // storage for all atoms (negative indices).
    // node index -1 refers to index 0 in this vector, -2 refers to 1 and so
    // on.
    atom_vec: Vec<AtomBuf>,
}

impl Default for Allocator {
    fn default() -> Self {
        Self::new()
    }
}

impl Allocator {
    pub fn new() -> Self {
        let mut r = Self {
            u8_vec: Vec::new(),
            pair_vec: Vec::new(),
            atom_vec: Vec::new(),
        };
        r.u8_vec.reserve(1024 * 1024);
        r.atom_vec.reserve(256);
        r.pair_vec.reserve(256);
        r.u8_vec.push(1_u8);
        // Preallocated empty list
        r.atom_vec.push(AtomBuf { start: 0, end: 0 });
        // Preallocated 1
        r.atom_vec.push(AtomBuf { start: 0, end: 1 });
        r
    }

    pub fn new_atom(&mut self, v: &[u8]) -> Result<NodePtr, EvalErr> {
        let start = self.u8_vec.len() as u32;
        if ((u32::MAX - start) as usize) < v.len() {
            return err(self.null(), "out of memory");
        }
        self.u8_vec.extend_from_slice(v);
        let end = self.u8_vec.len() as u32;
        if self.atom_vec.len() == i32::MAX as usize {
            return err(self.null(), "too many atoms");
        }
        self.atom_vec.push(AtomBuf { start, end });
        Ok(-(self.atom_vec.len() as i32))
    }

    pub fn new_pair(&mut self, first: NodePtr, rest: NodePtr) -> Result<NodePtr, EvalErr> {
        let r = self.pair_vec.len() as i32;
        if self.pair_vec.len() == i32::MAX as usize {
            return err(self.null(), "too many pairs");
        }
        self.pair_vec.push(IntPair { first, rest });
        Ok(r)
    }

    pub fn new_substr(&mut self, node: NodePtr, start: u32, end: u32) -> Result<NodePtr, EvalErr> {
        if node >= 0 {
            return err(node, "(internal error) substr expected atom, got pair");
        }
        let atom = self.atom_vec[(-node - 1) as usize];
        let atom_len = atom.end - atom.start;
        if start > atom_len {
            return err(node, "substr start out of bounds");
        }
        if end > atom_len {
            return err(node, "substr end out of bounds");
        }
        if end < start {
            return err(node, "substr invalid bounds");
        }
        self.atom_vec.push(AtomBuf {
            start: atom.start + start,
            end: atom.start + end,
        });
        Ok(-(self.atom_vec.len() as i32))
    }

    pub fn new_concat(&mut self, new_size: usize, nodes: &[NodePtr]) -> Result<NodePtr, EvalErr> {
        if self.atom_vec.len() == i32::MAX as usize {
            return err(self.null(), "too many atoms");
        }
        let start = self.u8_vec.len() as u32;
        if ((u32::MAX - start) as usize) < new_size {
            return err(self.null(), "out of memory");
        }
        self.u8_vec.reserve(new_size);

        let mut counter: usize = 0;
        for node in nodes {
            if *node >= 0 {
                self.u8_vec.truncate(start as usize);
                return err(*node, "(internal error) concat expected atom, got pair");
            }

            let term = self.atom_vec[(-node - 1) as usize];
            if counter + term.len() > new_size {
                self.u8_vec.truncate(start as usize);
                return err(*node, "(internal error) concat passed invalid new_size");
            }
            self.u8_vec
                .extend_from_within(term.start as usize..term.end as usize);
            counter += term.len();
        }
        if counter != new_size {
            self.u8_vec.truncate(start as usize);
            return err(
                self.null(),
                "(internal error) concat passed invalid new_size",
            );
        }
        let end = self.u8_vec.len() as u32;
        self.atom_vec.push(AtomBuf { start, end });
        Ok(-(self.atom_vec.len() as i32))
    }

    pub fn atom(&self, node: NodePtr) -> &[u8] {
        assert!(node < 0, "expected atom, got pair");
        let atom = self.atom_vec[(-node - 1) as usize];
        &self.u8_vec[atom.start as usize..atom.end as usize]
    }

    pub fn buf<'a>(&'a self, node: &AtomBuf) -> &'a [u8] {
        &self.u8_vec[node.start as usize..node.end as usize]
    }

    pub fn sexp(&self, node: NodePtr) -> SExp {
        if node >= 0 {
            let pair = self.pair_vec[node as usize];
            SExp::Pair(pair.first, pair.rest)
        } else {
            let atom = self.atom_vec[(-node - 1) as usize];
            SExp::Atom(atom)
        }
    }

    pub fn null(&self) -> NodePtr {
        -1
    }

    pub fn one(&self) -> NodePtr {
        -2
    }
}
