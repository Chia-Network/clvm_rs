use crate::allocator::Allocator;
use crate::chia_dialect::chia_dialect as rs_chia_dialect;
use crate::chia_dialect::OperatorHandlerWithMode;
use crate::cost::Cost;
use crate::dialect::Dialect as RsDialect;
use crate::py::adapt_response::adapt_response_to_py;
use crate::py::lazy_node::LazyNode;
use crate::serialize::node_from_bytes;

use pyo3::prelude::*;

#[pyfunction]
fn chia_dialect(strict: bool) -> Dialect {
    Dialect {
        dialect: rs_chia_dialect(strict),
    }
}

#[pyclass]
pub struct Dialect {
    dialect: RsDialect<OperatorHandlerWithMode>,
}

#[pymethods]
impl Dialect {
    pub fn deserialize_and_run_program(
        &self,
        py: Python,
        program_bytes: &[u8],
        arg_bytes: &[u8],
        max_cost: Cost,
    ) -> PyResult<(u64, LazyNode)> {
        let mut allocator = Allocator::new();
        let program = node_from_bytes(&mut allocator, program_bytes).unwrap();
        let args = node_from_bytes(&mut allocator, arg_bytes).unwrap();
        let r = self
            .dialect
            .run_program(&mut allocator, program, args, max_cost);
        adapt_response_to_py(py, allocator, r)
    }
}
