use crate::allocator::{Allocator, SExp};
use crate::arc_allocator::ArcAllocator;
use std::fmt::{self, Debug, Display, Formatter};
use std::sync::Arc;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

pub type U8 = Arc<[u8]>;

#[pyclass(subclass, unsendable)]
#[derive(Clone)]
pub struct Node {
    node: Arc<SExp<Node, U8>>,
}

fn extract_atom(allocator: &ArcAllocator, obj: &PyAny) -> PyResult<Node> {
    let r: &[u8] = obj.extract()?;
    Ok(allocator.blob_u8(r))
}

fn extract_node(_allocator: &ArcAllocator, obj: &PyAny) -> PyResult<Node> {
    let ps: &PyCell<Node> = obj.extract()?;
    let node: Node = ps.try_borrow()?.clone();
    Ok(node)
}

fn extract_tuple(allocator: &ArcAllocator, obj: &PyAny) -> PyResult<Node> {
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
        let allocator = ArcAllocator::new();
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
        let sexp: &SExp<Node, U8> = &self.node;
        match sexp {
            SExp::Pair(a, b) => Some((a.clone(), b.clone())),
            _ => None,
        }
    }

    #[getter(atom)]
    pub fn atom(&self) -> Option<&[u8]> {
        let sexp: &SExp<Node, U8> = &self.node;
        match sexp {
            SExp::Atom(a) => Some(a),
            _ => None,
        }
    }
}

impl Node {
    pub fn nullp(&self) -> bool {
        match self.atom() {
            Some(blob) => blob.is_empty(),
            None => false,
        }
    }

    pub fn sexp(&self) -> &SExp<Node, U8> {
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

impl From<Arc<SExp<Node, U8>>> for Node {
    fn from(item: Arc<SExp<Node, U8>>) -> Self {
        Node { node: item }
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
