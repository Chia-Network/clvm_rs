use std::fmt::{self, Debug, Display, Formatter};
use std::sync::Arc;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

pub type Atom = Box<[u8]>;

#[pyclass(subclass, unsendable)]
pub struct Allocator {}

#[derive(Debug, PartialEq)]
pub enum SExp {
    Atom(Atom),
    Pair(Node, Node),
}

#[pyclass(subclass, unsendable)]
#[derive(Clone, PartialEq)]
pub struct Node {
    node: Arc<SExp>,
}

fn extract_atom(obj: &PyAny) -> PyResult<Node> {
    let r: &[u8] = obj.extract()?;
    Ok(Node::blob_u8(r))
}

fn extract_node(obj: &PyAny) -> PyResult<Node> {
    let ps: &PyCell<Node> = obj.extract()?;
    let node: Node = ps.try_borrow()?.clone();
    Ok(node)
}

fn extract_tuple(obj: &PyAny) -> PyResult<Node> {
    let v: &PyTuple = obj.extract()?;
    if v.len() != 2 {
        return Err(PyValueError::new_err("SExp tuples must be size 2"));
    }
    let i0: &PyAny = v.get_item(0);
    let i1: &PyAny = v.get_item(1);
    let left: Node = extract_node(i0)?;
    let right: Node = extract_node(i1)?;
    let node: Node = Node::from_pair(&left, &right);
    Ok(node)
}

#[pymethods]
impl Node {
    #[new]
    pub fn new(obj: &PyAny) -> PyResult<Self> {
        let node: Node = {
            let n = extract_atom(obj);
            if let Ok(r) = n {
                r
            } else {
                extract_tuple(obj)?
            }
        };
        Ok(node)
    }

    #[getter(pair)]
    pub fn pair(&self) -> Option<(Node, Node)> {
        let sexp: &SExp = &self.node;
        match sexp {
            SExp::Pair(a, b) => Some((a.clone(), b.clone())),
            _ => None,
        }
    }

    #[getter(atom)]
    pub fn atom(&self) -> Option<&[u8]> {
        let sexp: &SExp = &self.node;
        match sexp {
            SExp::Atom(a) => Some(a),
            _ => None,
        }
    }
}

impl Node {
    pub fn null() -> Self {
        Node::blob("")
    }

    pub fn blob(v: &str) -> Self {
        Node {
            node: Arc::new(SExp::Atom(Vec::from(v).into())),
        }
    }

    pub fn blob_u8(v: &[u8]) -> Self {
        Node {
            node: Arc::new(SExp::Atom(Vec::from(v).into())),
        }
    }

    pub fn from_pair(first: &Node, rest: &Node) -> Self {
        Node {
            node: Arc::new(SExp::Pair(first.clone(), rest.clone())),
        }
    }

    pub fn is_pair(&self) -> bool {
        let sexp: &SExp = &self.node;
        matches!(sexp, SExp::Pair(_a, _b))
    }

    pub fn nullp(&self) -> bool {
        match self.atom() {
            Some(blob) => blob.is_empty(),
            None => false,
        }
    }

    pub fn sexp(&self) -> &SExp {
        &self.node
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
            SExp::Pair(first, rest) => {
                let v = first.clone();
                self.node = rest.node.clone();
                Some(v)
            }
            _ => None,
        }
    }
}

impl From<u8> for Node {
    fn from(item: u8) -> Self {
        let v: Vec<u8> = vec![item];
        Node::blob_u8(&v)
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
