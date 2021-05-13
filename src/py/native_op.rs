use pyo3::prelude::{pyclass, pymethods};
use pyo3::types::PyString;
use pyo3::{PyAny, PyObject, PyResult, Python, ToPyObject};

use crate::cost::Cost;
use crate::int_allocator::IntAllocator;
use crate::reduction::Reduction;

use super::error_bridge::raise_eval_error;
use super::f_table::OpFn;
use super::py_arena::PyArena;

//type OpFn<T> = fn(&mut T, <T as Allocator>::Ptr, Cost) -> Response<<T as Allocator>::Ptr>;

#[pyclass]
pub struct NativeOp {
    pub op: OpFn<IntAllocator>,
}

impl NativeOp {
    pub fn new(op: OpFn<IntAllocator>) -> Self {
        Self { op }
    }
}

#[pymethods]
impl NativeOp {
    #[call]
    fn __call__<'p>(
        &'p self,
        py: Python<'p>,
        args: &'p PyAny,
        _max_cost: Cost,
    ) -> PyResult<(Cost, PyObject)> {
        let arena_cell = PyArena::new_cell(py)?;
        let arena: &PyArena = &arena_cell.borrow();
        let ptr = arena.ptr_for_obj(py, args)?;
        let mut allocator = arena.allocator();
        let allocator: &mut IntAllocator = &mut allocator;
        let r = (self.op)(allocator, ptr, _max_cost);
        match r {
            Ok(Reduction(cost, ptr)) => {
                let r = arena.obj_for_ptr(py, ptr)?;
                Ok((cost, r.to_object(py)))
            }
            Err(_err) => {
                let r = arena.obj_for_ptr(py, ptr)?;
                match raise_eval_error(
                    py,
                    PyString::new(py, "problem in suboperator"),
                    r.to_object(py),
                ) {
                    Err(e) => Err(e),
                    Ok(_) => panic!("oh dear"),
                }
            }
        }
    }
}
