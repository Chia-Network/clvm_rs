use crate::err_utils::err;
use crate::number::{node_from_number, number_from_u8, Number};
use crate::reduction::EvalErr;
use bls12_381::{G1Affine, G1Projective, G2Affine, G2Projective};

pub type NodePtr = i32;

pub enum SExp {
    Atom(),
    Pair(NodePtr, NodePtr),
}

#[derive(Clone, Copy, Debug)]
struct AtomBuf {
    start: u32,
    end: u32,
}

impl AtomBuf {
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
        // if any of these asserts fire, it means we're trying to restore to
        // a state that has already been "long-jumped" passed (via another
        // restore to an earler state). You can only restore backwards in time,
        // not forwards.
        assert!(self.u8_vec.len() >= cp.u8s);
        assert!(self.pair_vec.len() >= cp.pairs);
        assert!(self.atom_vec.len() >= cp.atoms);
        self.u8_vec.truncate(cp.u8s);
        self.pair_vec.truncate(cp.pairs);
        self.atom_vec.truncate(cp.atoms);
    }

    pub fn new_atom(&mut self, v: &[u8]) -> Result<NodePtr, EvalErr> {
        let start = self.u8_vec.len() as u32;
        if (self.heap_limit - start as usize) < v.len() {
            return err(self.null(), "out of memory");
        }
        if self.atom_vec.len() == self.atom_limit {
            return err(self.null(), "too many atoms");
        }
        self.u8_vec.extend_from_slice(v);
        let end = self.u8_vec.len() as u32;
        self.atom_vec.push(AtomBuf { start, end });
        Ok(-(self.atom_vec.len() as i32))
    }

    pub fn new_number(&mut self, v: Number) -> Result<NodePtr, EvalErr> {
        node_from_number(self, &v)
    }

    pub fn new_g1(&mut self, g1: G1Projective) -> Result<NodePtr, EvalErr> {
        let g1: G1Affine = g1.into();
        self.new_atom(&g1.to_compressed())
    }

    pub fn new_g2(&mut self, g2: G2Projective) -> Result<NodePtr, EvalErr> {
        let g2: G2Affine = g2.into();
        self.new_atom(&g2.to_compressed())
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

    pub fn atom_eq(&self, lhs: NodePtr, rhs: NodePtr) -> bool {
        self.atom(lhs) == self.atom(rhs)
    }

    pub fn atom(&self, node: NodePtr) -> &[u8] {
        assert!(node < 0, "expected atom, got pair");
        let atom = self.atom_vec[(-node - 1) as usize];
        &self.u8_vec[atom.start as usize..atom.end as usize]
    }

    pub fn atom_len(&self, node: NodePtr) -> usize {
        self.atom(node).len()
    }

    pub fn number(&self, node: NodePtr) -> Number {
        number_from_u8(self.atom(node))
    }

    pub fn g1(&self, node: NodePtr) -> Result<G1Projective, EvalErr> {
        let blob = match self.sexp(node) {
            SExp::Atom() => self.atom(node),
            _ => {
                return err(node, "pair found, expected G1 point");
            }
        };
        if blob.len() != 48 {
            return err(node, "atom is not G1 size, 48 bytes");
        }

        let affine: Option<G1Affine> =
            G1Affine::from_compressed(blob.try_into().expect("G1 slice is not 48 bytes")).into();
        match affine {
            Some(point) => Ok(G1Projective::from(point)),
            None => err(node, "atom is not a G1 point"),
        }
    }

    pub fn g2(&self, node: NodePtr) -> Result<G2Projective, EvalErr> {
        let blob = match self.sexp(node) {
            SExp::Atom() => self.atom(node),
            _ => {
                return err(node, "pair found, expected G2 point");
            }
        };
        if blob.len() != 96 {
            return err(node, "atom is not G2 size, 96 bytes");
        }

        let affine: Option<G2Affine> =
            G2Affine::from_compressed(blob.try_into().expect("G2 slice is not 96 bytes")).into();
        match affine {
            Some(point) => Ok(G2Projective::from(point)),
            None => err(node, "atom is not a G2 point"),
        }
    }

    pub fn sexp(&self, node: NodePtr) -> SExp {
        if node >= 0 {
            let pair = self.pair_vec[node as usize];
            SExp::Pair(pair.first, pair.rest)
        } else {
            SExp::Atom()
        }
    }

    // this is meant to be used when iterating lists:
    // while let Some((i, rest)) = a.next(node) {
    //     node = rest;
    //     ...
    // }
    pub fn next(&self, n: NodePtr) -> Option<(NodePtr, NodePtr)> {
        match self.sexp(n) {
            SExp::Pair(first, rest) => Some((first, rest)),
            SExp::Atom() => None,
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
fn test_atom_eq() {
    let mut a = Allocator::new();
    let a0 = a.null();
    let a1 = a.one();
    let a2 = a.new_atom(&[1]).unwrap();
    let a3 = a.new_atom(&[0x5, 0x39]).unwrap();
    let a4 = a.new_number(1.into()).unwrap();
    let a5 = a.new_number(1337.into()).unwrap();

    assert!(a.atom_eq(a0, a0));
    assert!(!a.atom_eq(a0, a1));
    assert!(!a.atom_eq(a0, a2));
    assert!(!a.atom_eq(a0, a3));
    assert!(!a.atom_eq(a0, a4));
    assert!(!a.atom_eq(a0, a5));

    assert!(!a.atom_eq(a1, a0));
    assert!(a.atom_eq(a1, a1));
    assert!(a.atom_eq(a1, a2));
    assert!(!a.atom_eq(a1, a3));
    assert!(a.atom_eq(a1, a4));
    assert!(!a.atom_eq(a1, a5));

    assert!(!a.atom_eq(a2, a0));
    assert!(a.atom_eq(a2, a1));
    assert!(a.atom_eq(a2, a2));
    assert!(!a.atom_eq(a2, a3));
    assert!(a.atom_eq(a2, a4));
    assert!(!a.atom_eq(a2, a5));

    assert!(!a.atom_eq(a3, a0));
    assert!(!a.atom_eq(a3, a1));
    assert!(!a.atom_eq(a3, a2));
    assert!(a.atom_eq(a3, a3));
    assert!(!a.atom_eq(a3, a4));
    assert!(a.atom_eq(a3, a5));

    assert!(!a.atom_eq(a4, a0));
    assert!(a.atom_eq(a4, a1));
    assert!(a.atom_eq(a4, a2));
    assert!(!a.atom_eq(a4, a3));
    assert!(a.atom_eq(a4, a4));
    assert!(!a.atom_eq(a4, a5));
}

#[test]
fn test_null() {
    let a = Allocator::new();
    assert_eq!(a.atom(a.null()), b"");

    let buf = match a.sexp(a.null()) {
        SExp::Atom() => a.atom(a.null()),
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
            SExp::Atom() => a.atom(a.one()),
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
            SExp::Atom() => a.atom(atom),
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
            SExp::Atom() => panic!("unexpected"),
            SExp::Pair(left, right) => (left, right),
        },
        (atom1, atom2)
    );

    let pair2 = a.new_pair(pair, pair).unwrap();
    assert_eq!(
        match a.sexp(pair2) {
            SExp::Atom() => panic!("unexpected"),
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

    // the 4th fails and ensure not to append atom to the stack
    assert_eq!(a.u8_vec.len(), 10);
    assert_eq!(a.new_atom(b"foobar").unwrap_err().1, "too many atoms");
    assert_eq!(a.u8_vec.len(), 10);
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
            SExp::Atom() => 0,
            SExp::Pair(_, _) => 1,
        },
        0
    );
    assert_eq!(
        match a.sexp(atom2) {
            SExp::Atom() => 0,
            SExp::Pair(_, _) => 1,
        },
        0
    );
    assert_eq!(
        match a.sexp(pair) {
            SExp::Atom() => 0,
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

#[cfg(test)]
use rstest::rstest;

#[cfg(test)]
#[rstest]
#[case(0.into(), &[])]
#[case(1.into(), &[1])]
#[case((-1).into(), &[0xff])]
#[case(0x80.into(), &[0, 0x80])]
#[case(0xff.into(), &[0, 0xff])]
#[case(0xffffffff_u64.into(), &[0, 0xff, 0xff, 0xff, 0xff])]
fn test_new_number(#[case] num: Number, #[case] expected: &[u8]) {
    let mut a = Allocator::new();

    // TEST creating the atom from a Number
    let atom = a.new_number(num.clone()).unwrap();

    // make sure we get back the same number
    assert_eq!(a.number(atom), num);
    assert_eq!(a.atom(atom), expected);
    assert_eq!(number_from_u8(expected), num);

    // TEST creating the atom from a buffer
    let atom = a.new_atom(expected).unwrap();

    // make sure we get back the same number
    assert_eq!(a.number(atom), num);
    assert_eq!(a.atom(atom), expected);
    assert_eq!(number_from_u8(expected), num);
}

#[test]
fn test_checkpoints() {
    let mut a = Allocator::new();

    let atom1 = a.new_atom(&[1, 2, 3]).unwrap();
    assert!(a.atom(atom1) == [1, 2, 3]);

    let checkpoint = a.checkpoint();

    let atom2 = a.new_atom(&[4, 5, 6]).unwrap();
    assert!(a.atom(atom1) == [1, 2, 3]);
    assert!(a.atom(atom2) == [4, 5, 6]);

    // at this point we have two atoms and a checkpoint from before the second
    // atom was created

    // now, restoring the checkpoint state will make atom2 disappear

    a.restore_checkpoint(&checkpoint);

    assert!(a.atom(atom1) == [1, 2, 3]);
    let atom3 = a.new_atom(&[6, 7, 8]).unwrap();
    assert!(a.atom(atom3) == [6, 7, 8]);

    // since atom2 was removed, atom3 should actually be using that slot
    assert_eq!(atom2, atom3);
}

#[cfg(test)]
fn test_g1(a: &Allocator, n: NodePtr) -> EvalErr {
    a.g1(n).unwrap_err()
}

#[cfg(test)]
fn test_g2(a: &Allocator, n: NodePtr) -> EvalErr {
    a.g2(n).unwrap_err()
}

#[cfg(test)]
type TestFun = fn(&Allocator, NodePtr) -> EvalErr;

#[cfg(test)]
#[rstest]
#[case(test_g1, 0, "atom is not G1 size, 48 bytes")]
#[case(test_g1, 3, "atom is not G1 size, 48 bytes")]
#[case(test_g1, 47, "atom is not G1 size, 48 bytes")]
#[case(test_g1, 49, "atom is not G1 size, 48 bytes")]
#[case(test_g1, 48, "atom is not a G1 point")]
#[case(test_g2, 0, "atom is not G2 size, 96 bytes")]
#[case(test_g2, 3, "atom is not G2 size, 96 bytes")]
#[case(test_g2, 95, "atom is not G2 size, 96 bytes")]
#[case(test_g2, 97, "atom is not G2 size, 96 bytes")]
#[case(test_g2, 96, "atom is not a G2 point")]
fn test_point_size_error(#[case] fun: TestFun, #[case] size: usize, #[case] expected: &str) {
    let mut a = Allocator::new();
    let mut buf = Vec::<u8>::new();
    buf.resize(size, 0xcc);
    let n = a.new_atom(&buf).unwrap();
    let r = fun(&a, n);
    assert_eq!(r.0, n);
    assert_eq!(r.1, expected.to_string());
}

#[cfg(test)]
#[rstest]
#[case(test_g1, "pair found, expected G1 point")]
#[case(test_g2, "pair found, expected G2 point")]
fn test_point_atom_pair(#[case] fun: TestFun, #[case] expected: &str) {
    let mut a = Allocator::new();
    let n = a.new_pair(a.null(), a.one()).unwrap();
    let r = fun(&a, n);
    assert_eq!(r.0, n);
    assert_eq!(r.1, expected.to_string());
}

#[cfg(test)]
#[rstest]
#[case(
    "\
97f1d3a73197d7942695638c4fa9ac0f\
c3688c4f9774b905a14e3a3f171bac58\
6c55e83ff97a1aeffb3af00adb22c6bb"
)]
#[case(
    "\
a572cbea904d67468808c8eb50a9450c\
9721db309128012543902d0ac358a62a\
e28f75bb8f1c7c42c39a8c5529bf0f4e"
)]
fn test_g1_roundtrip(#[case] atom: &str) {
    let mut a = Allocator::new();
    let n = a.new_atom(&hex::decode(atom).unwrap()).unwrap();
    let g1 = a.g1(n).unwrap();
    assert_eq!(hex::encode(G1Affine::from(g1).to_compressed()), atom);

    let g1_copy = a.new_g1(g1).unwrap();
    let g1_atom = a.atom(g1_copy);
    assert_eq!(hex::encode(g1_atom), atom);

    // try interpreting the point as G1
    assert_eq!(a.g2(n).unwrap_err().1, "atom is not G2 size, 96 bytes");
    assert_eq!(
        a.g2(g1_copy).unwrap_err().1,
        "atom is not G2 size, 96 bytes"
    );

    // try interpreting the point as number
    assert_eq!(a.number(n), number_from_u8(&hex::decode(atom).unwrap()));
    assert_eq!(
        a.number(g1_copy),
        number_from_u8(&hex::decode(atom).unwrap())
    );
}

#[cfg(test)]
#[rstest]
#[case(
    "\
93e02b6052719f607dacd3a088274f65\
596bd0d09920b61ab5da61bbdc7f5049\
334cf11213945d57e5ac7d055d042b7e\
024aa2b2f08f0a91260805272dc51051\
c6e47ad4fa403b02b4510b647ae3d177\
0bac0326a805bbefd48056c8c121bdb8"
)]
#[case(
    "\
aa4edef9c1ed7f729f520e47730a124f\
d70662a904ba1074728114d1031e1572\
c6c886f6b57ec72a6178288c47c33577\
1638533957d540a9d2370f17cc7ed586\
3bc0b995b8825e0ee1ea1e1e4d00dbae\
81f14b0bf3611b78c952aacab827a053"
)]
fn test_g2_roundtrip(#[case] atom: &str) {
    let mut a = Allocator::new();
    let n = a.new_atom(&hex::decode(atom).unwrap()).unwrap();
    let g2 = a.g2(n).unwrap();
    assert_eq!(hex::encode(G2Affine::from(g2).to_compressed()), atom);

    let g2_copy = a.new_g2(g2).unwrap();
    let g2_atom = a.atom(g2_copy);
    assert_eq!(hex::encode(g2_atom), atom);

    // try interpreting the point as G1
    assert_eq!(a.g1(n).unwrap_err().1, "atom is not G1 size, 48 bytes");
    assert_eq!(
        a.g1(g2_copy).unwrap_err().1,
        "atom is not G1 size, 48 bytes"
    );

    // try interpreting the point as number
    assert_eq!(a.number(n), number_from_u8(&hex::decode(atom).unwrap()));
    assert_eq!(
        a.number(g2_copy),
        number_from_u8(&hex::decode(atom).unwrap())
    );
}

#[cfg(test)]
use core::convert::TryFrom;

#[cfg(test)]
type MakeFun = fn(&mut Allocator, &[u8]) -> NodePtr;

#[cfg(test)]
fn make_buf(a: &mut Allocator, bytes: &[u8]) -> NodePtr {
    a.new_atom(bytes).unwrap()
}

#[cfg(test)]
fn make_number(a: &mut Allocator, bytes: &[u8]) -> NodePtr {
    let v = number_from_u8(bytes);
    a.new_number(v).unwrap()
}

#[cfg(test)]
fn make_g1(a: &mut Allocator, bytes: &[u8]) -> NodePtr {
    let v: G1Projective = G1Affine::from_compressed(bytes.try_into().unwrap())
        .unwrap()
        .into();
    a.new_g1(v).unwrap()
}

#[cfg(test)]
fn make_g2(a: &mut Allocator, bytes: &[u8]) -> NodePtr {
    let v: G2Projective = G2Affine::from_compressed(bytes.try_into().unwrap())
        .unwrap()
        .into();
    a.new_g2(v).unwrap()
}

#[cfg(test)]
fn make_g1_fail(a: &mut Allocator, bytes: &[u8]) -> NodePtr {
    assert!(<[u8; 48]>::try_from(bytes).is_err());
    //assert!(G1Affine::from_compressed(bytes.try_into().unwrap()).is_none().unwrap_u8() != 0);
    a.new_atom(bytes).unwrap()
}

#[cfg(test)]
fn make_g2_fail(a: &mut Allocator, bytes: &[u8]) -> NodePtr {
    assert!(<[u8; 96]>::try_from(bytes).is_err());
    //assert!(G2Affine::from_compressed(bytes.try_into().unwrap()).is_none().unwrap_u8() != 0);
    a.new_atom(bytes).unwrap()
}

#[cfg(test)]
type CheckFun = fn(&Allocator, NodePtr, &[u8]);

#[cfg(test)]
fn check_buf(a: &Allocator, n: NodePtr, bytes: &[u8]) {
    let buf = a.atom(n);
    assert_eq!(buf, bytes);
}

#[cfg(test)]
fn check_number(a: &Allocator, n: NodePtr, bytes: &[u8]) {
    let num = a.number(n);
    let v = number_from_u8(bytes);
    assert_eq!(num, v);
}

#[cfg(test)]
fn check_g1(a: &Allocator, n: NodePtr, bytes: &[u8]) {
    let num = a.g1(n).unwrap();
    let v: G1Projective = G1Affine::from_compressed(bytes.try_into().unwrap())
        .unwrap()
        .into();
    assert_eq!(num, v);
}

#[cfg(test)]
fn check_g2(a: &Allocator, n: NodePtr, bytes: &[u8]) {
    let num = a.g2(n).unwrap();
    let v: G2Projective = G2Affine::from_compressed(bytes.try_into().unwrap())
        .unwrap()
        .into();
    assert_eq!(num, v);
}

#[cfg(test)]
fn check_g1_fail(a: &Allocator, n: NodePtr, bytes: &[u8]) {
    assert_eq!(a.g1(n).unwrap_err().0, n);
    //assert!(G1Affine::from_compressed(bytes.try_into().unwrap()).is_none().unwrap_u8() != 0);
    assert!(<[u8; 48]>::try_from(bytes).is_err());
}

#[cfg(test)]
fn check_g2_fail(a: &Allocator, n: NodePtr, bytes: &[u8]) {
    assert_eq!(a.g2(n).unwrap_err().0, n);
    //assert!(G2Affine::from_compressed(bytes.try_into().unwrap()).is_none().unwrap_u8() != 0);
    assert!(<[u8; 96]>::try_from(bytes).is_err());
}

#[cfg(test)]
const EMPTY: &str = "";

#[cfg(test)]
const SMALL_BUF: &str = "133742";

#[cfg(test)]
const VALID_G1: &str = "\
a572cbea904d67468808c8eb50a9450c\
9721db309128012543902d0ac358a62a\
e28f75bb8f1c7c42c39a8c5529bf0f4e";

#[cfg(test)]
const VALID_G2: &str = "\
aa4edef9c1ed7f729f520e47730a124f\
d70662a904ba1074728114d1031e1572\
c6c886f6b57ec72a6178288c47c33577\
1638533957d540a9d2370f17cc7ed586\
3bc0b995b8825e0ee1ea1e1e4d00dbae\
81f14b0bf3611b78c952aacab827a053";

/*
  We want to exercise round-tripping avery kind of value via every other kind
  of value (as far as possible). e.g. Every value can round-trip through a byte buffer
  or a number, but G1 cannot round-trip via G2.

  +-----------+--------+--------+------+------+
  | from / to | buffer | number | G1   | G2   |
  +-----------+--------+--------+------+------+
  | buffer    | o      | o      | -    | -    |
  | number    | o      | o      | -    | -    |
  | G1        | o      | o      | o    | -    |
  | G2        | o      | o      | -    | o    |
  +-----------+--------+--------+------+------+

*/

#[cfg(test)]
#[rstest]
// round trip empty buffer
#[case(EMPTY, make_buf, check_buf)]
#[case(EMPTY, make_buf, check_number)]
#[case(EMPTY, make_buf, check_g1_fail)]
#[case(EMPTY, make_buf, check_g2_fail)]
#[case(EMPTY, make_number, check_buf)]
#[case(EMPTY, make_number, check_number)]
#[case(EMPTY, make_number, check_g1_fail)]
#[case(EMPTY, make_number, check_g2_fail)]
#[case(EMPTY, make_g1_fail, check_buf)]
#[case(EMPTY, make_g1_fail, check_number)]
#[case(EMPTY, make_g1_fail, check_g1_fail)]
#[case(EMPTY, make_g1_fail, check_g2_fail)]
#[case(EMPTY, make_g2_fail, check_buf)]
#[case(EMPTY, make_g2_fail, check_number)]
#[case(EMPTY, make_g2_fail, check_g1_fail)]
#[case(EMPTY, make_g2_fail, check_g2_fail)]
// round trip small buffer
#[case(SMALL_BUF, make_buf, check_buf)]
#[case(SMALL_BUF, make_buf, check_number)]
#[case(SMALL_BUF, make_buf, check_g1_fail)]
#[case(SMALL_BUF, make_buf, check_g2_fail)]
#[case(SMALL_BUF, make_number, check_buf)]
#[case(SMALL_BUF, make_number, check_number)]
#[case(SMALL_BUF, make_number, check_g1_fail)]
#[case(SMALL_BUF, make_number, check_g2_fail)]
#[case(SMALL_BUF, make_g1_fail, check_buf)]
#[case(SMALL_BUF, make_g1_fail, check_number)]
#[case(SMALL_BUF, make_g1_fail, check_g1_fail)]
#[case(SMALL_BUF, make_g1_fail, check_g2_fail)]
#[case(SMALL_BUF, make_g2_fail, check_buf)]
#[case(SMALL_BUF, make_g2_fail, check_number)]
#[case(SMALL_BUF, make_g2_fail, check_g1_fail)]
#[case(SMALL_BUF, make_g2_fail, check_g2_fail)]
// round trip G1 point
#[case(VALID_G1, make_buf, check_buf)]
#[case(VALID_G1, make_buf, check_number)]
#[case(VALID_G1, make_buf, check_g1)]
#[case(VALID_G1, make_buf, check_g2_fail)]
#[case(VALID_G1, make_number, check_buf)]
#[case(VALID_G1, make_number, check_number)]
#[case(VALID_G1, make_number, check_g1)]
#[case(VALID_G1, make_number, check_g2_fail)]
#[case(VALID_G1, make_g1, check_buf)]
#[case(VALID_G1, make_g1, check_number)]
#[case(VALID_G1, make_g1, check_g1)]
#[case(VALID_G1, make_g1, check_g2_fail)]
#[case(VALID_G1, make_g2_fail, check_buf)]
#[case(VALID_G1, make_g2_fail, check_number)]
#[case(VALID_G1, make_g2_fail, check_g1)]
#[case(VALID_G1, make_g2_fail, check_g2_fail)]
// round trip G2 point
#[case(VALID_G2, make_buf, check_buf)]
#[case(VALID_G2, make_buf, check_number)]
#[case(VALID_G2, make_buf, check_g1_fail)]
#[case(VALID_G2, make_buf, check_g2)]
#[case(VALID_G2, make_number, check_buf)]
#[case(VALID_G2, make_number, check_number)]
#[case(VALID_G2, make_number, check_g1_fail)]
#[case(VALID_G2, make_number, check_g2)]
#[case(VALID_G2, make_g1_fail, check_buf)]
#[case(VALID_G2, make_g1_fail, check_number)]
#[case(VALID_G2, make_g1_fail, check_g1_fail)]
#[case(VALID_G2, make_g1_fail, check_g2)]
#[case(VALID_G2, make_g2, check_buf)]
#[case(VALID_G2, make_g2, check_number)]
#[case(VALID_G2, make_g2, check_g1_fail)]
#[case(VALID_G2, make_g2, check_g2)]
fn test_roundtrip(#[case] test_value: &str, #[case] make: MakeFun, #[case] check: CheckFun) {
    let value = hex::decode(test_value).unwrap();
    let mut a = Allocator::new();
    let node = make(&mut a, &value);
    check(&a, node, &value);
}

#[cfg(test)]
#[rstest]
#[case(&[], 0)]
#[case(&[1], 1)]
#[case(&[1,2], 2)]
#[case(&[1,2,3,4,5,6,7,8,9], 9)]
#[case(&[1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18], 18)]
fn test_atom_len(#[case] buf: &[u8], #[case] expected: usize) {
    let mut a = Allocator::new();
    let atom = a.new_atom(buf).unwrap();
    assert_eq!(a.atom_len(atom), expected);
}

#[cfg(test)]
#[rstest]
#[case(0.into(), 0)]
#[case(42.into(), 1)]
#[case(127.into(), 1)]
#[case(1337.into(), 2)]
#[case(0x7fffff.into(), 3)]
#[case(0xffffff.into(), 4)]
#[case((-1).into(), 1)]
#[case((-128).into(), 1)]
fn test_atom_len_number(#[case] value: Number, #[case] expected: usize) {
    let mut a = Allocator::new();
    let atom = a.new_number(value).unwrap();
    assert_eq!(a.atom_len(atom), expected);
}

#[cfg(test)]
#[rstest]
#[case(
    "\
97f1d3a73197d7942695638c4fa9ac0f\
c3688c4f9774b905a14e3a3f171bac58\
6c55e83ff97a1aeffb3af00adb22c6bb",
    48
)]
#[case(
    "\
a572cbea904d67468808c8eb50a9450c\
9721db309128012543902d0ac358a62a\
e28f75bb8f1c7c42c39a8c5529bf0f4e",
    48
)]
fn test_atom_len_g1(#[case] buffer_hex: &str, #[case] expected: usize) {
    let mut a = Allocator::new();
    let buffer = &hex::decode(buffer_hex).unwrap();
    let g1 =
        G1Projective::from(G1Affine::from_compressed(&buffer[..].try_into().unwrap()).unwrap());
    let atom = a.new_g1(g1).unwrap();
    assert_eq!(a.atom_len(atom), expected);
}

#[cfg(test)]
#[rstest]
#[case(
    "\
93e02b6052719f607dacd3a088274f65\
596bd0d09920b61ab5da61bbdc7f5049\
334cf11213945d57e5ac7d055d042b7e\
024aa2b2f08f0a91260805272dc51051\
c6e47ad4fa403b02b4510b647ae3d177\
0bac0326a805bbefd48056c8c121bdb8",
    96
)]
#[case(
    "\
aa4edef9c1ed7f729f520e47730a124f\
d70662a904ba1074728114d1031e1572\
c6c886f6b57ec72a6178288c47c33577\
1638533957d540a9d2370f17cc7ed586\
3bc0b995b8825e0ee1ea1e1e4d00dbae\
81f14b0bf3611b78c952aacab827a053",
    96
)]
fn test_atom_len_g2(#[case] buffer_hex: &str, #[case] expected: usize) {
    let mut a = Allocator::new();

    let buffer = &hex::decode(buffer_hex).unwrap();
    let g2 =
        G2Projective::from(G2Affine::from_compressed(&buffer[..].try_into().unwrap()).unwrap());
    let atom = a.new_g2(g2).unwrap();
    assert_eq!(a.atom_len(atom), expected);
}
