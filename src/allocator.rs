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

// this represents a specific (former) state of an allocator. This can be used
// to restore an allocator to a previous state. It cannot be used to re-create
// the state from some other allocator.
pub struct Checkpoint {
    u8s: usize,
    pairs: usize,
    atoms: usize,
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

    // the atom_vec may not grow past this
    heap_limit: usize,

    // the pair_vec may not grow past this
    pair_limit: usize,

    // the atom_vec may not grow past this
    atom_limit: usize,
}

impl Default for Allocator {
    fn default() -> Self {
        Self::new()
    }
}

impl Allocator {
    pub fn new() -> Self {
        Self::new_limited(
            u32::MAX as usize,
            i32::MAX as usize,
            (i32::MAX - 1) as usize,
        )
    }

    pub fn new_limited(heap_limit: usize, pair_limit: usize, atom_limit: usize) -> Self {
        // we have a maximum of 4 GiB heap, because pointers are 32 bit unsigned
        assert!(heap_limit <= u32::MAX as usize);
        // the atoms and pairs share a single 32 bit address space, where
        // negative numbers are atoms and positive numbers are pairs. That's why
        // we have one more slot for pairs than atoms
        assert!(pair_limit <= i32::MAX as usize);
        assert!(atom_limit < i32::MAX as usize);

        let mut r = Self {
            u8_vec: Vec::new(),
            pair_vec: Vec::new(),
            atom_vec: Vec::new(),
            heap_limit,
            pair_limit,
            atom_limit,
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

    // create a checkpoint for the current state of the allocator. This can be
    // used to go back to an earlier allocator state by passing the Checkpoint
    // to restore_checkpoint().
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            u8s: self.u8_vec.len(),
            pairs: self.pair_vec.len(),
            atoms: self.atom_vec.len(),
        }
    }

    pub fn restore_checkpoint(&mut self, cp: &Checkpoint) {
        self.u8_vec.truncate(cp.u8s);
        self.pair_vec.truncate(cp.pairs);
        self.atom_vec.truncate(cp.atoms);
    }

    pub fn new_atom(&mut self, v: &[u8]) -> Result<NodePtr, EvalErr> {
        let start = self.u8_vec.len() as u32;
        if (self.heap_limit - start as usize) < v.len() {
            return err(self.null(), "out of memory");
        }
        self.u8_vec.extend_from_slice(v);
        let end = self.u8_vec.len() as u32;
        if self.atom_vec.len() == self.atom_limit {
            return err(self.null(), "too many atoms");
        }
        self.atom_vec.push(AtomBuf { start, end });
        Ok(-(self.atom_vec.len() as i32))
    }

    pub fn new_pair(&mut self, first: NodePtr, rest: NodePtr) -> Result<NodePtr, EvalErr> {
        let r = self.pair_vec.len() as i32;
        if self.pair_vec.len() == self.pair_limit {
            return err(self.null(), "too many pairs");
        }
        self.pair_vec.push(IntPair { first, rest });
        Ok(r)
    }

    pub fn new_substr(&mut self, node: NodePtr, start: u32, end: u32) -> Result<NodePtr, EvalErr> {
        if node >= 0 {
            return err(node, "(internal error) substr expected atom, got pair");
        }
        if self.atom_vec.len() == self.atom_limit {
            return err(self.null(), "too many atoms");
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
        if self.atom_vec.len() == self.atom_limit {
            return err(self.null(), "too many atoms");
        }
        let start = self.u8_vec.len();
        if self.heap_limit - start < new_size {
            return err(self.null(), "out of memory");
        }
        self.u8_vec.reserve(new_size);

        let mut counter: usize = 0;
        for node in nodes {
            if *node >= 0 {
                self.u8_vec.truncate(start);
                return err(*node, "(internal error) concat expected atom, got pair");
            }

            let term = self.atom_vec[(-node - 1) as usize];
            if counter + term.len() > new_size {
                self.u8_vec.truncate(start);
                return err(*node, "(internal error) concat passed invalid new_size");
            }
            self.u8_vec
                .extend_from_within(term.start as usize..term.end as usize);
            counter += term.len();
        }
        if counter != new_size {
            self.u8_vec.truncate(start);
            return err(
                self.null(),
                "(internal error) concat passed invalid new_size",
            );
        }
        let end = self.u8_vec.len() as u32;
        self.atom_vec.push(AtomBuf {
            start: (start as u32),
            end,
        });
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

    #[cfg(feature = "counters")]
    pub fn atom_count(&self) -> usize {
        self.atom_vec.len()
    }

    #[cfg(feature = "counters")]
    pub fn pair_count(&self) -> usize {
        self.pair_vec.len()
    }

    #[cfg(feature = "counters")]
    pub fn heap_size(&self) -> usize {
        self.u8_vec.len()
    }
}

#[test]
fn test_null() {
    let a = Allocator::new();
    assert_eq!(a.atom(a.null()), b"");

    let buf = match a.sexp(a.null()) {
        SExp::Atom(b) => a.buf(&b),
        SExp::Pair(_, _) => panic!("unexpected"),
    };
    assert_eq!(buf, b"");
}

#[test]
fn test_one() {
    let a = Allocator::new();
    assert_eq!(a.atom(a.one()), b"\x01");
    assert_eq!(
        match a.sexp(a.one()) {
            SExp::Atom(b) => a.buf(&b),
            SExp::Pair(_, _) => panic!("unexpected"),
        },
        b"\x01"
    );
}

#[test]
fn test_allocate_atom() {
    let mut a = Allocator::new();
    let atom = a.new_atom(b"foobar").unwrap();
    assert_eq!(a.atom(atom), b"foobar");
    assert_eq!(
        match a.sexp(atom) {
            SExp::Atom(b) => a.buf(&b),
            SExp::Pair(_, _) => panic!("unexpected"),
        },
        b"foobar"
    );
}

#[test]
fn test_allocate_pair() {
    let mut a = Allocator::new();
    let atom1 = a.new_atom(b"foo").unwrap();
    let atom2 = a.new_atom(b"bar").unwrap();
    let pair = a.new_pair(atom1, atom2).unwrap();

    assert_eq!(
        match a.sexp(pair) {
            SExp::Atom(_) => panic!("unexpected"),
            SExp::Pair(left, right) => (left, right),
        },
        (atom1, atom2)
    );

    let pair2 = a.new_pair(pair, pair).unwrap();
    assert_eq!(
        match a.sexp(pair2) {
            SExp::Atom(_) => panic!("unexpected"),
            SExp::Pair(left, right) => (left, right),
        },
        (pair, pair)
    );
}

#[test]
fn test_allocate_heap_limit() {
    let mut a = Allocator::new_limited(6, i32::MAX as usize, (i32::MAX - 1) as usize);
    // we can't allocate 6 bytes
    assert_eq!(a.new_atom(b"foobar").unwrap_err().1, "out of memory");
    // but 5 is OK
    let _atom = a.new_atom(b"fooba").unwrap();
}

#[test]
fn test_allocate_atom_limit() {
    let mut a = Allocator::new_limited(u32::MAX as usize, i32::MAX as usize, 5);
    // we can allocate 5 atoms total
    // keep in mind that we always have 2 pre-allocated atoms for null and one,
    // so with a limit of 5, we only have 3 slots left at this point.
    let _atom = a.new_atom(b"foo").unwrap();
    let _atom = a.new_atom(b"bar").unwrap();
    let _atom = a.new_atom(b"baz").unwrap();

    // the 4th fails
    assert_eq!(a.new_atom(b"foobar").unwrap_err().1, "too many atoms");
}

#[test]
fn test_allocate_pair_limit() {
    let mut a = Allocator::new_limited(u32::MAX as usize, 1, (i32::MAX - 1) as usize);
    let atom = a.new_atom(b"foo").unwrap();
    // one pair is OK
    let _pair1 = a.new_pair(atom, atom).unwrap();
    // but not 2
    assert_eq!(a.new_pair(atom, atom).unwrap_err().1, "too many pairs");
}

#[test]
fn test_substr() {
    let mut a = Allocator::new();
    let atom = a.new_atom(b"foobar").unwrap();

    let sub = a.new_substr(atom, 0, 1).unwrap();
    assert_eq!(a.atom(sub), b"f");
    let sub = a.new_substr(atom, 1, 6).unwrap();
    assert_eq!(a.atom(sub), b"oobar");
    let sub = a.new_substr(atom, 1, 1).unwrap();
    assert_eq!(a.atom(sub), b"");
    let sub = a.new_substr(atom, 0, 0).unwrap();
    assert_eq!(a.atom(sub), b"");

    assert_eq!(
        a.new_substr(atom, 1, 0).unwrap_err().1,
        "substr invalid bounds"
    );
    assert_eq!(
        a.new_substr(atom, 7, 7).unwrap_err().1,
        "substr start out of bounds"
    );
    assert_eq!(
        a.new_substr(atom, 0, 7).unwrap_err().1,
        "substr end out of bounds"
    );
    assert_eq!(
        a.new_substr(atom, u32::MAX, 4).unwrap_err().1,
        "substr start out of bounds"
    );
}

#[test]
fn test_concat() {
    let mut a = Allocator::new();
    let atom1 = a.new_atom(b"f").unwrap();
    let atom2 = a.new_atom(b"o").unwrap();
    let atom3 = a.new_atom(b"o").unwrap();
    let atom4 = a.new_atom(b"b").unwrap();
    let atom5 = a.new_atom(b"a").unwrap();
    let atom6 = a.new_atom(b"r").unwrap();
    let pair = a.new_pair(atom1, atom2).unwrap();

    let cat = a
        .new_concat(6, &[atom1, atom2, atom3, atom4, atom5, atom6])
        .unwrap();
    assert_eq!(a.atom(cat), b"foobar");

    let cat = a.new_concat(12, &[cat, cat]).unwrap();
    assert_eq!(a.atom(cat), b"foobarfoobar");

    assert_eq!(
        a.new_concat(11, &[cat, cat]).unwrap_err().1,
        "(internal error) concat passed invalid new_size"
    );
    assert_eq!(
        a.new_concat(13, &[cat, cat]).unwrap_err().1,
        "(internal error) concat passed invalid new_size"
    );
    assert_eq!(
        a.new_concat(12, &[atom3, pair]).unwrap_err().1,
        "(internal error) concat expected atom, got pair"
    );
}

#[test]
fn test_sexp() {
    let mut a = Allocator::new();
    let atom1 = a.new_atom(b"f").unwrap();
    let atom2 = a.new_atom(b"o").unwrap();
    let pair = a.new_pair(atom1, atom2).unwrap();

    assert_eq!(
        match a.sexp(atom1) {
            SExp::Atom(_) => 0,
            SExp::Pair(_, _) => 1,
        },
        0
    );
    assert_eq!(
        match a.sexp(atom2) {
            SExp::Atom(_) => 0,
            SExp::Pair(_, _) => 1,
        },
        0
    );
    assert_eq!(
        match a.sexp(pair) {
            SExp::Atom(_) => 0,
            SExp::Pair(_, _) => 1,
        },
        1
    );
}

#[test]
fn test_concat_limit() {
    let mut a = Allocator::new_limited(9, i32::MAX as usize, (i32::MAX - 1) as usize);
    let atom1 = a.new_atom(b"f").unwrap();
    let atom2 = a.new_atom(b"o").unwrap();
    let atom3 = a.new_atom(b"o").unwrap();
    let atom4 = a.new_atom(b"b").unwrap();
    let atom5 = a.new_atom(b"a").unwrap();
    let atom6 = a.new_atom(b"r").unwrap();

    // we only have 2 bytes left of allowed heap allocation
    assert_eq!(
        a.new_concat(6, &[atom1, atom2, atom3, atom4, atom5, atom6])
            .unwrap_err()
            .1,
        "out of memory"
    );
    let cat = a.new_concat(2, &[atom1, atom2]).unwrap();
    assert_eq!(a.atom(cat), b"fo");
}

#[test]
fn test_checkpoints() {
    let mut a = Allocator::new();

    let atom1 = a.new_atom(&[1, 2, 3]).unwrap();
    assert!(a.atom(atom1) == &[1, 2, 3]);

    let checkpoint = a.checkpoint();

    let atom2 = a.new_atom(&[4, 5, 6]).unwrap();
    assert!(a.atom(atom1) == &[1, 2, 3]);
    assert!(a.atom(atom2) == &[4, 5, 6]);

    // at this point we have two atoms and a checkpoint from before the second
    // atom was created

    // now, restoring the checkpoint state will make atom2 disappear

    a.restore_checkpoint(&checkpoint);

    assert!(a.atom(atom1) == &[1, 2, 3]);
    let atom3 = a.new_atom(&[6, 7, 8]).unwrap();
    assert!(a.atom(atom3) == &[6, 7, 8]);

    // since atom2 was removed, atom3 should actually be using that slot
    assert_eq!(atom2, atom3);
}
