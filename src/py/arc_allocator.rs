use crate::allocator::{Allocator, SExp};

use std::sync::Arc;

use lazy_static::*;

use pyo3::prelude::*;

#[pyclass(subclass, unsendable)]
#[derive(Clone)]
pub struct ArcAllocator {}

#[derive(Clone)]
pub struct ArcAtomBuf {
    buf: Arc<Vec<u8>>,
    start: u32,
    end: u32,
}

pub enum ArcSExp {
    Atom(ArcAtomBuf),
    Pair(Arc<ArcSExp>, Arc<ArcSExp>),
}

lazy_static! {
    static ref NULL: Arc<Vec<u8>> = Arc::new(vec![]);
    static ref ONE: Arc<Vec<u8>> = Arc::new(vec![1]);
}

impl Clone for ArcSExp {
    fn clone(&self) -> Self {
        match self {
            ArcSExp::Atom(a) => ArcSExp::Atom(a.clone()),
            ArcSExp::Pair(p1, p2) => ArcSExp::Pair(p1.clone(), p2.clone()),
        }
    }
}

impl ArcAllocator {
    pub fn new() -> Self {
        ArcAllocator {}
    }

    pub fn blob(&mut self, v: &str) -> ArcSExp {
        let v: Vec<u8> = v.into();
        self.new_atom(&v)
    }
}

impl Allocator for ArcAllocator {
    type Ptr = ArcSExp;
    type AtomBuf = ArcAtomBuf;

    fn new_atom(&mut self, v: &[u8]) -> ArcSExp {
        ArcSExp::Atom(ArcAtomBuf {
            buf: Arc::new(v.into()),
            start: 0,
            end: v.len() as u32,
        })
    }

    fn new_pair(&mut self, first: ArcSExp, rest: ArcSExp) -> ArcSExp {
        ArcSExp::Pair(Arc::new(first), Arc::new(rest))
    }

    fn new_substr(&mut self, node: Self::Ptr, start: u32, end: u32) -> Self::Ptr {
        let atom = match node {
            ArcSExp::Atom(a) => a,
            _ => panic!("substr expected atom, got pair"),
        };
        let atom_len = atom.end - atom.start;
        if start > atom_len {
            panic!("substr start out of bounds");
        }
        if end > atom_len {
            panic!("substr end out of bounds");
        }
        if end < start {
            panic!("substr invalid bounds");
        }
        ArcSExp::Atom(ArcAtomBuf {
            buf: atom.buf.clone(),
            start: atom.start,
            end: atom.end,
        })
    }

    fn atom<'a>(&'a self, node: &'a Self::Ptr) -> &'a [u8] {
        match node {
            ArcSExp::Atom(a) => &a.buf[a.start as usize..a.end as usize],
            _ => panic!("expected atom, got pair"),
        }
    }

    fn buf<'a>(&'a self, node: &'a Self::AtomBuf) -> &'a [u8] {
        &node.buf[node.start as usize..node.end as usize]
    }

    fn sexp(&self, node: &ArcSExp) -> SExp<ArcSExp, ArcAtomBuf> {
        match node {
            ArcSExp::Atom(a) => SExp::Atom(a.clone()),
            ArcSExp::Pair(left, right) => {
                let p1: &ArcSExp = &left;
                let p2: &ArcSExp = &right;
                SExp::Pair(p1.to_owned(), p2.to_owned())
            }
        }
    }

    fn null(&self) -> ArcSExp {
        ArcSExp::Atom(ArcAtomBuf {
            buf: NULL.to_owned(),
            start: 0,
            end: 0,
        })
    }

    fn one(&self) -> ArcSExp {
        ArcSExp::Atom(ArcAtomBuf {
            buf: ONE.to_owned(),
            start: 0,
            end: 1,
        })
    }
}

impl Default for ArcAllocator {
    fn default() -> Self {
        Self::new()
    }
}
