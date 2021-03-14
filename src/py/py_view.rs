use pyo3::pycell::PyCell;
use pyo3::types::{PyBytes, PyTuple};
use pyo3::{PyObject, PyResult, Python, ToPyObject};

use super::py_na_node::PyNaNode;

#[derive(Clone)]
pub enum PyView {
    Atom(PyObject),
    Pair(PyObject),
}

impl PyView {
    pub fn new_atom(py: Python, atom: &PyBytes) -> Self {
        PyView::Atom(atom.to_object(py))
    }

    pub fn new_pair(py: Python, pair: &PyTuple) -> PyResult<Self> {
        if pair.len() != 2 {
            py.eval("raise ValueError('new_pair requires 2-tuple')", None, None)?;
        }
        let _p0: &PyCell<PyNaNode> = pair.get_item(0).extract()?;
        let _p1: &PyCell<PyNaNode> = pair.get_item(1).extract()?;

        Ok(PyView::Pair(pair.to_object(py)))
    }

    fn atom<'p>(&'p self, py: Python<'p>) -> Option<&'p PyBytes> {
        match self {
            PyView::Atom(obj) => Some(obj.extract(py).unwrap()),
            _ => None,
        }
    }

    fn pair<'p>(&'p self, py: Python<'p>) -> PyResult<Option<&'p PyTuple>> {
        match self {
            PyView::Pair(obj) => Ok(Some(obj.extract(py)?)),
            _ => Ok(None),
        }
    }

    /*
    fn pair_as_cells(&self, py: Python) -> PyResult<Option<(&PyCell<PyNode>, &PyCell<PyNode>)>> {
        let pair = self.pair(py)?;
        let p0: &'p PyCell<PyNode> = pair.get_item(0).extract()?;
        let p1: &'p PyCell<PyNode> = pair.get_item(1).extract()?;
        Ok((p0, p1))
    }
    */
}
