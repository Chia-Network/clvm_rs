use std::fmt::{self, Debug, Display, Formatter};
use std::sync::Arc;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

#[pyclass(subclass, unsendable)]
pub struct Allocator {}

static ONE: [u8; 1] = [1];

pub trait AllocatorTrait<T> {
    fn blob_u8(&self, v: &[u8]) -> T;
    fn from_pair(&self, first: &T, rest: &T) -> T;
    fn sexp(&self, node: &T) -> &SExp<T>;
}

pub enum SExp<'a, NodeT> {
    Atom(&'a [u8]),
    Pair(&'a NodeT, &'a NodeT),
}

impl Allocator {
    pub fn null(&self) -> Node {
        self.blob("")
    }

    pub fn one(&self) -> Node {
        self.blob_u8(&ONE)
    }

    pub fn blob(&self, v: &str) -> Node {
        Node {
            node: Arc::new(SExpN::Atom(Vec::from(v).into())),
        }
    }

    pub fn blob_u8(&self, v: &[u8]) -> Node {
        Node {
            node: Arc::new(SExpN::Atom(Vec::from(v).into())),
        }
    }

    pub fn from_pair(&self, first: &Node, rest: &Node) -> Node {
        Node {
            node: Arc::new(SExpN::Pair(first.clone(), rest.clone())),
        }
    }

    pub fn sexp<'a>(&self, node: &'a Node) -> SExp<'a, Node> {
        match &*node.node {
            SExpN::Atom(a) => SExp::Atom(&a),
            SExpN::Pair(left, right) => SExp::Pair(&left, &right),
        }
    }
}

#[derive(Debug, PartialEq)]
enum SExpN {
    Atom(Arc<Vec<u8>>),
    Pair(Node, Node),
}

#[pyclass(subclass, unsendable)]
#[derive(Clone, PartialEq)]
pub struct Node {
    node: Arc<SExpN>,
}

fn extract_atom(allocator: &Allocator, obj: &PyAny) -> PyResult<Node> {
    let r: &[u8] = obj.extract()?;
    Ok(allocator.blob_u8(r))
}

fn extract_node(allocator: &Allocator, obj: &PyAny) -> PyResult<Node> {
    let ps: &PyCell<Node> = obj.extract()?;
    let node: Node = ps.try_borrow()?.clone();
    Ok(node)
}

fn extract_tuple(allocator: &Allocator, obj: &PyAny) -> PyResult<Node> {
    let v: &PyTuple = obj.extract()?;
    if v.len() != 2 {
        return Err(PyValueError::new_err("SExp tuples must be size 2"));
    }
    let i0: &PyAny = v.get_item(0);
    let i1: &PyAny = v.get_item(1);
    let left: Node = extract_node(&allocator, i0)?;
    let right: Node = extract_node(&allocator, i1)?;
    let node: Node = allocator.from_pair(&left, &right);
    Ok(node)
}

#[pymethods]
impl Node {
    #[new]
    pub fn new(obj: &PyAny) -> PyResult<Self> {
        let allocator = Allocator {};
        let node: Node = {
            let n = extract_atom(&allocator, obj);
            if let Ok(r) = n {
                r
            } else {
                extract_tuple(&allocator, obj)?
            }
        };
        Ok(node)
    }

    #[getter(pair)]
    pub fn pair(&self) -> Option<(Node, Node)> {
        let sexp: &SExpN = &self.node;
        match sexp {
            SExpN::Pair(a, b) => Some((a.clone(), b.clone())),
            _ => None,
        }
    }

    #[getter(atom)]
    pub fn atom(&self) -> Option<&[u8]> {
        let sexp: &SExpN = &self.node;
        match sexp {
            SExpN::Atom(a) => Some(a),
            _ => None,
        }
    }
}

impl Node {
    pub fn is_pair(&self) -> bool {
        let sexp: &SExpN = &self.node;
        matches!(sexp, SExpN::Pair(_a, _b))
    }

    pub fn nullp(&self) -> bool {
        match self.atom() {
            Some(blob) => blob.is_empty(),
            None => false,
        }
    }

    fn fmt_list(&self, f: &mut Formatter, is_first: bool) -> fmt::Result {
        if let Some((first, rest)) = self.pair() {
            if !is_first {
                write!(f, " ")?;
            }
            Display::fmt(&first, f)?;
            rest.fmt_list(f, false)
        } else {
            if !self.nullp() {
                write!(f, " . ")?;
                self.fmt_list(f, false)?;
            }
            Ok(())
        }
    }
}

impl Display for Node {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if let Some(blob) = self.atom() {
            let t: &[u8] = &*blob;
            if t.is_empty() {
                write!(f, "()")?;
            } else {
                write!(f, "0x")?;
                for u in t {
                    write!(f, "{:02x}", u)?;
                }
            }
        }
        if let Some((_first, _rest)) = self.pair() {
            write!(f, "(")?;
            self.fmt_list(f, true)?;
            write!(f, ")")?;
        }

        Ok(())
    }
}

impl Debug for Node {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if let Some(blob) = self.atom() {
            let t: &[u8] = &*blob;
            if t.is_empty() {
                write!(f, "()")?;
            } else {
                write!(f, "0x")?;
                for u in t {
                    write!(f, "{:02x}", u)?;
                }
            }
        }
        if let Some((_first, _rest)) = self.pair() {
            write!(f, "(")?;
            self.fmt_list(f, true)?;
            write!(f, ")")?;
        }

        Ok(())
    }
}

impl Iterator for Node {
    type Item = Node;

    fn next(&mut self) -> Option<Self::Item> {
        match &*self.node {
            SExpN::Pair(first, rest) => {
                let v = first.clone();
                self.node = rest.node.clone();
                Some(v)
            }
            _ => None,
        }
    }
}

impl From<Node> for Option<u8> {
    fn from(item: Node) -> Option<u8> {
        let blob = item.atom()?;
        let len = blob.len();
        if len == 0 {
            Some(0)
        } else if len == 1 {
            Some(blob[0])
        } else {
            None
        }
    }
}
