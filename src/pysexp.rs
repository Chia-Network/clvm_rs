use super::node::Node;

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

#[pyclass(subclass)]
pub struct PySExp {
    pub node: Node,
}

fn extract_atom(obj: &PyAny) -> PyResult<Node> {
    let r: &[u8] = obj.extract()?;
    Ok(Node::blob_u8(r))
}

fn extract_node(obj: &PyAny) -> PyResult<Node> {
    let ps: PyRef<PySExp> = obj.extract()?;
    let node: Node = ps.node.clone();
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
    let node: Node = Node::pair(&left, &right);
    Ok(node)
}

#[pymethods]
impl PySExp {
    #[new]
    fn new(obj: &PyAny) -> PyResult<Self> {
        let node: Node = {
            let other = extract_node(obj);
            match other {
                Ok(n) => n,
                _ => {
                    let n = extract_atom(obj);
                    if let Ok(r) = n {
                        r
                    } else {
                        extract_tuple(obj)?
                    }
                }
            }
        };
        Ok(node.into())
    }

    #[getter(pair)]
    pub fn pair(&self) -> Option<(PySExp, PySExp)> {
        self.as_pair()
    }
    pub fn as_pair(&self) -> Option<(PySExp, PySExp)> {
        match self.node.as_pair() {
            None => None,
            Some(pair) => {
                let left = pair.0;
                let right = pair.1;
                let new_pair = (left.into(), right.into());
                Some(new_pair)
            }
        }
    }

    #[getter(atom)]
    pub fn atom(&self) -> Option<&[u8]> {
        self.as_atom()
    }
    pub fn as_atom(&self) -> Option<&[u8]> {
        self.node.as_blob()
    }

    pub fn listp(&self) -> bool {
        self.node.is_pair()
    }

    pub fn nullp(&self) -> bool {
        self.node.nullp()
    }

    pub fn cons(&self, other: &PySExp) -> PyResult<PySExp> {
        let node = Node::pair(&self.node, &other.node);
        Ok(PySExp { node })
    }
}

impl From<Node> for PySExp {
    fn from(item: Node) -> PySExp {
        PySExp { node: item }
    }
}

impl IntoPy<PySExp> for Node {
    fn into_py(self, py: Python<'_>) -> PySExp {
        PySExp { node: self }
    }
}

/*
impl From<&<'a> PySExp> for &<'a> Node {
    fn from(self) -> &Node {
        &self.node
    }
}

*/
