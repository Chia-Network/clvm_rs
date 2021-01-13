use super::arc_allocator::ArcAllocator;
use crate::allocator::Allocator;
use std::cell::RefCell;
use std::fmt::{self, Debug, Display, Formatter};
use std::sync::Arc;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyTuple};

pub enum PySExp {
    Atom(Arc<[u8]>),
    Pair(Arc<PyNode>, Arc<PyNode>),
}

#[pyclass(subclass, unsendable)]
#[derive(Clone)]
pub struct PyNode {
    node: Arc<PySExp>,
    bytes: RefCell<Option<PyObject>>,
}

fn extract_atom(_allocator: &ArcAllocator, obj: &PyAny) -> PyResult<PyNode> {
    let py_bytes: &PyBytes = obj.extract()?;
    let r: &[u8] = obj.extract()?;
    let r1: Arc<[u8]> = r.into();
    let inner_node: Arc<PySExp> = Arc::new(PySExp::Atom(r1));
    let py_node = PyNode {
        node: inner_node,
        bytes: RefCell::new(Some(py_bytes.into())),
    };
    //println!("py_bytes is {:?}", py_node.bytes);
    Ok(py_node)
}

fn extract_node(_allocator: &ArcAllocator, obj: &PyAny) -> PyResult<PyNode> {
    let ps: &PyCell<PyNode> = obj.extract()?;
    let node: PyNode = ps.try_borrow()?.clone();
    Ok(node)
}

fn extract_tuple(allocator: &ArcAllocator, obj: &PyAny) -> PyResult<PyNode> {
    let v: &PyTuple = obj.extract()?;
    if v.len() != 2 {
        return Err(PyValueError::new_err("SExp tuples must be size 2"));
    }
    let i0: &PyAny = v.get_item(0);
    let i1: &PyAny = v.get_item(1);
    let left: PyNode = extract_node(&allocator, i0)?;
    let right: PyNode = extract_node(&allocator, i1)?;
    let node: PyNode = allocator.new_pair(&left, &right);
    Ok(node)
}

#[pymethods]
impl PyNode {
    #[new]
    pub fn new(obj: &PyAny) -> PyResult<Self> {
        let allocator = ArcAllocator::new();
        let node: PyNode = {
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
    pub fn pair(&self) -> Option<(PyNode, PyNode)> {
        self._pair()
    }

    pub fn _pair(&self) -> Option<(PyNode, PyNode)> {
        let sexp: &PySExp = &self.node;
        match sexp {
            PySExp::Pair(a, b) => Some((a.into(), b.into())),
            _ => None,
        }
    }

    #[getter(atom)]
    pub fn atom<'p>(&self, py: Python<'p>) -> Option<PyObject> {
        let sexp: &PySExp = &self.node;
        match sexp {
            PySExp::Atom(_a) => {
                {
                    let mut borrowed_bytes = self.bytes.borrow_mut();
                    if borrowed_bytes.is_none() {
                        let b: &PyBytes = PyBytes::new(py, _a);
                        let obj: PyObject = b.into();
                        *borrowed_bytes = Some(obj);
                    };
                }
                self.bytes.borrow().clone()
            }
            _ => None,
        }
    }

    pub fn _atom(&self) -> Option<&[u8]> {
        let sexp: &PySExp = &self.node;
        match sexp {
            PySExp::Atom(a) => Some(a),
            _ => None,
        }
    }
}

impl PyNode {
    pub fn nullp(&self) -> bool {
        match self._atom() {
            Some(blob) => blob.is_empty(),
            None => false,
        }
    }

    pub fn sexp(&self) -> &PySExp {
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

impl Display for PyNode {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if let Some(blob) = self._atom() {
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

impl Debug for PyNode {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if let Some(blob) = self._atom() {
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

impl From<&Arc<PySExp>> for PyNode {
    fn from(item: &Arc<PySExp>) -> Self {
        item.clone().into()
    }
}

impl From<Arc<PySExp>> for PyNode {
    fn from(item: Arc<PySExp>) -> Self {
        PyNode {
            node: item,
            bytes: RefCell::new(None),
        }
    }
}

impl From<&Arc<PyNode>> for PyNode {
    fn from(item: &Arc<PyNode>) -> Self {
        PyNode {
            node: item.node.clone(),
            bytes: RefCell::new(None),
        }
    }
}

fn move_out(n: &mut Arc<PySExp>) -> Arc<PySExp> {
    // for some reason rust doesn't let us move members out of self when being
    // destructed, so we need to replace it with a dummy allocation
    std::mem::replace(n, Arc::new(PySExp::Atom(Arc::new([0]))))
}

// to avoid stack overflow by destructing the tree of nodes recursively, first
// move out all pointers with refcount 1 (get_mut() fails if there are multiple
// references) and stick them in a (shallow) vector. That way, destruction will
// happen sequentially, rather than recursively
impl Drop for PyNode {
    fn drop(&mut self) {
        if Arc::strong_count(&self.node) > 1 {
            return;
        };
        let mut vec = Vec::<Arc<PySExp>>::new();

        let mut current = move_out(&mut self.node);
        loop {
            let (left, right) = match Arc::get_mut(&mut current) {
                Some(PySExp::Pair(left, right)) => (Arc::get_mut(left), Arc::get_mut(right)),
                _ => (None, None),
            };

            if let Some(v) = left {
                vec.push(move_out(&mut v.node));
            }
            if let Some(v) = right {
                vec.push(move_out(&mut v.node));
            }
            current = match vec.pop() {
                Some(n) => n,
                _ => {
                    break;
                }
            };
        }
    }
}
