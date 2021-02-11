use crate::allocator::{Allocator, SExp};
use crate::reduction::EvalErr;

use std::sync::Arc;

use lazy_static::*;

use pyo3::prelude::*;

#[pyclass(subclass, unsendable)]
#[derive(Clone)]
pub struct ArcAllocator {}

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
        ArcAllocator {}
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

    fn new_pair(&self, first: ArcSExp, rest: ArcSExp) -> ArcSExp {
        ArcSExp::Pair(Arc::new(first), Arc::new(rest))
    }

    fn sexp<'a: 'c, 'b: 'c, 'c>(&'a self, node: &'b ArcSExp) -> SExp<'c, ArcSExp> {
        match node {
            ArcSExp::Atom(atom) => SExp::Atom(&atom),
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

impl<P> dyn Allocator<Ptr = P>
where
    P: Clone,
{
    pub fn err<T>(&self, node: &P, msg: &str) -> Result<T, EvalErr<P>> {
        Err(EvalErr(node.clone(), msg.into()))
    }
}
