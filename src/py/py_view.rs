use pyo3::{PyAny, PyObject, PyResult};

#[derive(Clone)]
pub enum PyView {
    Atom(PyObject),
    Pair(PyObject),
}

impl PyView {
    // TODO: move this into `py_arena` and use very similar `SExp<'p, &'p PyBytes, &'p PyAny>` instead
    pub fn py_view_for_obj(obj: &PyAny) -> PyResult<PyView> {
        let r: &PyAny = obj.getattr("atom")?.extract()?;
        if !r.is_none() {
            return Ok(PyView::Atom(r.into()));
        }
        let r: &PyAny = obj.getattr("pair")?.extract()?;
        Ok(PyView::Pair(r.into()))
    }
}
