use std::cell::RefMut;
use std::collections::HashMap;

use pyo3::prelude::{pyclass, pymethods};

use pyo3::types::{PyBytes, PyDict, PyString, PyTuple};
use pyo3::{PyCell, PyErr, PyObject, PyRef, PyResult, Python, ToPyObject};

use crate::allocator::Allocator;
use crate::cost::Cost;
use crate::err_utils::err;
use crate::int_allocator::IntAllocator;
use crate::more_ops::op_unknown;
use crate::reduction::EvalErr;
use crate::reduction::Reduction;
use crate::reduction::Response;
use crate::run_program::OperatorHandler;

use super::f_table::FLookup;
use super::f_table::OpFn;
use super::native_op::NativeOp;
use super::py_int_allocator::PyIntAllocator;
use super::py_node::PyNode;

#[pyclass]
pub struct Dialect {
    quote_kw: u8,
    apply_kw: u8,
    u8_lookup: FLookup<IntAllocator>,
    python_u8_lookup: HashMap<Vec<u8>, PyObject>,
    native_u8_lookup: HashMap<Vec<u8>, OpFn<IntAllocator>>,
    strict: bool,
}

#[pymethods]
impl Dialect {
    #[new]
    pub fn new(
        py: Python,
        quote_kw: u8,
        apply_kw: u8,
        op_table: HashMap<Vec<u8>, PyObject>,
        strict: bool,
    ) -> PyResult<Self> {
        let mut u8_lookup = [None; 256];
        let mut python_u8_lookup = HashMap::new();
        let mut native_u8_lookup = HashMap::new();
        for (op, fn_obj) in op_table.iter() {
            let r: PyResult<PyRef<NativeOp>> = fn_obj.extract(py);
            if let Ok(native_op) = r {
                if op.len() == 1 {
                    let index = op[0] as usize;
                    u8_lookup[index] = Some(native_op.op);
                } else {
                    native_u8_lookup.insert(op.to_owned(), native_op.op);
                }
            } else {
                python_u8_lookup.insert(op.to_owned(), fn_obj.clone());
            }
        }
        Ok(Self {
            quote_kw,
            apply_kw,
            u8_lookup,
            python_u8_lookup,
            native_u8_lookup,
            strict,
        })
    }

    pub fn run_program<'p>(
        &self,
        py: Python<'p>,
        program: &PyCell<PyNode>,
        args: &PyCell<PyNode>,
        max_cost: Cost,
    ) -> PyResult<(Cost, PyObject)> {
        let py_int_allocator = PyIntAllocator::new(py)?;
        let py_int_allocator = &py_int_allocator.borrow() as &PyIntAllocator;
        let drc = DialectRunningContext {
            dialect: self,
            py_int_allocator,
        };

        let mut allocator_refcell: RefMut<IntAllocator> = py_int_allocator.allocator();
        let allocator: &mut IntAllocator = &mut allocator_refcell as &mut IntAllocator;

        let program = py_int_allocator.native_for_py(py, program, allocator)?;
        let args = py_int_allocator.native_for_py(py, args, allocator)?;

        let r: Result<Reduction<i32>, EvalErr<i32>> = crate::run_program::run_program(
            allocator,
            &program,
            &args,
            self.quote_kw,
            self.apply_kw,
            max_cost,
            &drc,
            None,
        );

        match r {
            Ok(reduction) => {
                let r = py_int_allocator.py_for_native(py, &reduction.1, allocator)?;
                Ok((reduction.0, r.to_object(py)))
            }
            Err(eval_err) => {
                let node: PyObject = py_int_allocator
                    .py_for_native(py, &eval_err.0, allocator)?
                    .to_object(py);
                let s: String = eval_err.1;
                let s1: &str = &s;
                let msg: &PyString = PyString::new(py, s1);
                match raise_eval_error(py, &msg, node) {
                    Err(x) => Err(x),
                    _ => panic!(),
                }
            }
        }
    }
}

struct DialectRunningContext<'a> {
    dialect: &'a Dialect,
    py_int_allocator: &'a PyIntAllocator,
}

impl DialectRunningContext<'_> {
    pub fn invoke_py_obj(
        &self,
        obj: &PyObject,
        allocator: &mut IntAllocator,
        op_buf: <IntAllocator as Allocator>::AtomBuf,
        args: &<IntAllocator as Allocator>::Ptr,
        max_cost: Cost,
    ) -> Response<<IntAllocator as Allocator>::Ptr> {
        Python::with_gil(|py| {
            let op: &PyBytes = PyBytes::new(py, allocator.buf(&op_buf));
            let r = unwrap_or_eval_err(
                self.py_int_allocator.py_for_native(py, args, allocator),
                args,
                "can't uncache",
            )?;
            let r1 = obj.call1(py, (op, r.to_object(py), max_cost));
            match r1 {
                Err(pyerr) => {
                    let eval_err: PyResult<EvalErr<i32>> =
                        eval_err_for_pyerr(py, &pyerr, self.py_int_allocator, allocator);
                    let r: EvalErr<i32> =
                        unwrap_or_eval_err(eval_err, args, "unexpected exception")?;
                    Err(r)
                }
                Ok(o) => {
                    let pair: &PyTuple = unwrap_or_eval_err(o.extract(py), args, "expected tuple")?;

                    let i0: u32 =
                        unwrap_or_eval_err(pair.get_item(0).extract(), args, "expected u32")?;

                    let py_node: &PyCell<PyNode> =
                        unwrap_or_eval_err(pair.get_item(1).extract(), args, "expected node")?;

                    let r = self.py_int_allocator.native_for_py(py, py_node, allocator);
                    let node: i32 = unwrap_or_eval_err(r, args, "can't find in int allocator")?;
                    Ok(Reduction(i0 as Cost, node))
                }
            }
        })
    }
}

impl OperatorHandler<IntAllocator> for DialectRunningContext<'_> {
    fn op(
        &self,
        allocator: &mut IntAllocator,
        o: <IntAllocator as Allocator>::AtomBuf,
        argument_list: &<IntAllocator as Allocator>::Ptr,
        max_cost: Cost,
    ) -> Response<<IntAllocator as Allocator>::Ptr> {
        let op = &allocator.buf(&o);
        if op.len() == 1 {
            if let Some(f) = self.dialect.u8_lookup[op[0] as usize] {
                return f(allocator, *argument_list, max_cost);
            }
        }
        let op = op.to_owned();
        if let Some(op_fn) = self.dialect.native_u8_lookup.get(op) {
            op_fn(allocator, *argument_list, max_cost)
        } else if let Some(op_fn) = self.dialect.python_u8_lookup.get(op) {
            self.invoke_py_obj(op_fn, allocator, o, argument_list, max_cost)
        } else if self.dialect.strict {
            let buf = op.to_vec();
            let op_arg = allocator.new_atom(&buf)?;
            err(op_arg, "unimplemented operator")
        } else {
            op_unknown(allocator, o, *argument_list, max_cost)
        }
    }
}

/// turn a `PyErr` into an `EvalErr<P>` if at all possible
/// otherwise, return a `PyErr`
fn eval_err_for_pyerr<'p>(
    py: Python<'p>,
    pyerr: &PyErr,
    py_int_allocator: &'p PyIntAllocator,
    allocator: &mut IntAllocator,
) -> PyResult<EvalErr<i32>> {
    let args: &PyTuple = pyerr.pvalue(py).getattr("args")?.extract()?;
    let arg0: &PyString = args.get_item(0).extract()?;
    let sexp: &PyCell<PyNode> = pyerr.pvalue(py).getattr("_sexp")?.extract()?;
    let node: i32 = py_int_allocator.native_for_py(py, sexp, allocator)?;
    let s: String = arg0.to_str()?.to_string();
    Ok(EvalErr(node, s))
}

fn unwrap_or_eval_err<T, P>(obj: PyResult<T>, err_node: &P, msg: &str) -> Result<T, EvalErr<P>>
where
    P: Clone,
{
    match obj {
        Err(_py_err) => Err(EvalErr(err_node.clone(), msg.to_string())),
        Ok(o) => Ok(o),
    }
}

fn raise_eval_error(py: Python, msg: &PyString, sexp: PyObject) -> PyResult<PyObject> {
    let ctx: &PyDict = PyDict::new(py);
    ctx.set_item("msg", msg)?;
    ctx.set_item("sexp", sexp)?;
    let r = py.run(
        "from clvm.EvalError import EvalError; raise EvalError(msg, sexp)",
        None,
        Some(ctx),
    );
    match r {
        Err(x) => Err(x),
        Ok(_) => Ok(ctx.into()),
    }
}
