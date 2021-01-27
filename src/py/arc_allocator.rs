use crate::allocator::{Allocator, SExp};
use crate::reduction::EvalErr;

use std::sync::Arc;

use aovec::Aovec;

use lazy_static::*;

use pyo3::prelude::*;

#[pyclass(subclass, unsendable)]
pub struct ArcAllocator {
    vec: Aovec<Arc<Vec<u8>>>,
}

pub enum ArcSExp {
    Atom(Arc<Vec<u8>>),
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
        ArcAllocator {
            vec: Aovec::new(16),
        }
    }

    pub fn blob(&self, v: &str) -> ArcSExp {
        let v: Vec<u8> = v.into();
        self.new_atom(&v)
    }
}

impl Allocator for ArcAllocator {
    type Ptr = ArcSExp;

    fn new_atom(&self, v: &[u8]) -> ArcSExp {
        ArcSExp::Atom(Arc::new(v.into()))
    }

    fn new_pair(&self, first: &ArcSExp, rest: &ArcSExp) -> ArcSExp {
        ArcSExp::Pair(Arc::new(first.to_owned()), Arc::new(rest.to_owned()))
    }

    fn sexp(&self, node: &ArcSExp) -> SExp<ArcSExp> {
        match node {
            ArcSExp::Atom(atom) => {
                let idx = self.vec.len();
                self.vec.push(atom.to_owned());
                SExp::Atom(self.vec.get(idx).unwrap())
            }
            ArcSExp::Pair(left, right) => {
                let p1: &ArcSExp = &left;
                let p2: &ArcSExp = &right;
                SExp::Pair(p1.to_owned(), p2.to_owned())
            }
        }
    }

    fn null(&self) -> ArcSExp {
        let a = NULL.to_owned();
        ArcSExp::Atom(a)
    }

    fn one(&self) -> ArcSExp {
        let a = ONE.to_owned();
        ArcSExp::Atom(a)
    }
}

impl Default for ArcAllocator {
    fn default() -> Self {
        Self::new()
    }
}

impl ArcAllocator {
    pub fn err<T>(&self, node: &ArcSExp, msg: &str) -> Result<T, EvalErr<ArcSExp>> {
        Err(EvalErr(node.to_owned(), msg.into()))
    }
}
