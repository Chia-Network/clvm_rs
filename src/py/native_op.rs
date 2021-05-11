use pyo3::prelude::{pyclass, pymethods};
use pyo3::types::PyString;
use pyo3::{PyAny, PyCell, PyResult, Python, ToPyObject};

use crate::allocator::Allocator;
use crate::cost::Cost;
use crate::int_allocator::IntAllocator;
use crate::reduction::Reduction;

use super::arena_object::ArenaObject;
use super::error_bridge::{eval_err_for_pyerr, raise_eval_error};
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
    fn __call__(&self, py: Python, args: &PyAny, _max_cost: Cost) -> PyResult<(Cost, ArenaObject)> {
        let arena_cell = PyArena::new_cell(py)?;
        let ptr = PyArena::include(arena_cell, py, args)?.borrow().get_ptr();
        let arena: &PyArena = &arena_cell.borrow();
        let mut allocator = arena.allocator();
        let allocator: &mut IntAllocator = &mut allocator;
        let r = (self.op)(allocator, ptr, _max_cost);
        match r {
            Ok(Reduction(cost, ptr)) => {
                let r = ArenaObject::new(py, arena_cell, ptr);
                Ok((cost, r))
            }
            Err(err) => {
                let r = ArenaObject::new(py, arena_cell, ptr);
                match raise_eval_error(
                    py,
                    PyString::new(py, "problem in suboperator"),
                    PyCell::new(py, r)?.to_object(py),
                ) {
                    Err(e) => Err(e),
                    Ok(_) => panic!("oh dear"),
                }
            }
        }
    }
}
