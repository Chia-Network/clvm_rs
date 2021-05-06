use std::cell::RefMut;
use std::collections::HashMap;

use pyo3::prelude::{pyclass, pymethods};

use pyo3::types::{PyBytes, PyDict, PyString, PyTuple};
use pyo3::{FromPyObject, PyCell, PyErr, PyObject, PyRef, PyResult, Python, ToPyObject};

use crate::allocator::Allocator;
use crate::cost::Cost;
use crate::int_allocator::IntAllocator;
use crate::reduction::EvalErr;
use crate::reduction::Reduction;
use crate::reduction::Response;
use crate::run_program::OperatorHandler;
use crate::serialize::node_from_bytes;

use super::arena_object::ArenaObject;
use super::clvm_object::CLVMObject;
use super::f_table::FLookup;
use super::f_table::OpFn;
use super::native_op::NativeOp;
use super::py_arena::PyArena;

#[pyclass]
#[derive(Clone)]
pub struct PyMultiOpFn {
    op: MultiOpFn<IntAllocator>,
}

impl PyMultiOpFn {
    pub fn new(op: MultiOpFn<IntAllocator>) -> Self {
        Self { op }
    }
}

pub type MultiOpFn<T> = fn(
    &mut T,
    <T as Allocator>::AtomBuf,
    <T as Allocator>::Ptr,
    Cost,
) -> Response<<T as Allocator>::Ptr>;

#[derive(Clone)]
pub enum MultiOpFnE<T: Allocator> {
    Python(PyObject),
    Rust(MultiOpFn<T>),
}

impl<T: Allocator> MultiOpFnE<T> {
    pub fn invoke(
        &self,
        allocator: &mut T,
        o: <T as Allocator>::AtomBuf,
        args: <T as Allocator>::Ptr,
        max_cost: Cost,
    ) -> Response<<T as Allocator>::Ptr> {
        match self {
            Self::Python(_o) => {
                todo!()
            }
            Self::Rust(f) => f(allocator, o, args, max_cost),
        }
    }
}

impl<'source> FromPyObject<'source> for MultiOpFnE<IntAllocator> {
    fn extract(obj: &'source pyo3::PyAny) -> PyResult<Self> {
        let v: PyResult<&PyCell<PyMultiOpFn>> = obj.extract();
        if let Ok(v) = v {
            Ok(Self::Rust(v.borrow().op))
        } else {
            Ok(Self::Python(obj.into()))
        }
    }
}

fn same_arena(arena1: &PyArena, arena2: &PyArena) -> bool {
    let p1: *const PyArena = arena1 as *const PyArena;
    let p2: *const PyArena = arena2 as *const PyArena;
    p1 == p2
}

#[pyclass]
pub struct Dialect {
    quote_kw: u8,
    apply_kw: u8,
    u8_lookup: FLookup<IntAllocator>,
    python_u8_lookup: HashMap<Vec<u8>, PyObject>,
    native_u8_lookup: HashMap<Vec<u8>, OpFn<IntAllocator>>,
    unknown_op_callback: MultiOpFnE<IntAllocator>,
}

#[pymethods]
impl Dialect {
    #[new]
    pub fn new(
        py: Python,
        quote_kw: u8,
        apply_kw: u8,
        op_table: HashMap<Vec<u8>, PyObject>,
        unknown_op_callback: MultiOpFnE<IntAllocator>,
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
            unknown_op_callback,
        })
    }

    pub fn run_program<'p>(
        &self,
        py: Python<'p>,
        program: &PyCell<CLVMObject>,
        args: &PyCell<CLVMObject>,
        max_cost: Cost,
    ) -> PyResult<(Cost, PyObject)> {
        let arena = PyArena::new_cell(py)?;
        let arena_ptr: &PyArena = &arena.borrow() as &PyArena;

        let program = arena_ptr.ptr_for_obj(py, program)?;
        let args = arena_ptr.ptr_for_obj(py, args)?;

        let (cost, r) = self.run_program_ptr(py, &arena, program, args, max_cost)?;

        let mut allocator_refcell: RefMut<IntAllocator> = arena_ptr.allocator();
        let allocator: &mut IntAllocator = &mut allocator_refcell as &mut IntAllocator;

        let r_ptr = &(&r).into();
        let new_r = arena_ptr.py_for_native(py, r_ptr, allocator)?;
        Ok((cost, new_r.into()))
    }

    pub fn run_program_arena<'p>(
        &self,
        py: Python<'p>,
        program: &ArenaObject,
        args: &ArenaObject,
        max_cost: Cost,
    ) -> PyResult<(Cost, ArenaObject)> {
        let arena = program.get_arena(py)?;
        if !same_arena(&arena.borrow(), &args.get_arena(py)?.borrow()) {
            py.eval("raise ValueError('mismatched arenas')", None, None)?;
        }
        self.run_program_ptr(py, arena, program.into(), args.into(), max_cost)
    }

    pub fn deserialize_and_run_program<'p>(
        &self,
        py: Python<'p>,
        program_blob: &[u8],
        args_blob: &[u8],
        max_cost: Cost,
    ) -> PyResult<(Cost, ArenaObject)> {
        let arena = PyArena::new_cell(py)?;
        let (program, args) = {
            let arena_ptr: &PyArena = &arena.borrow() as &PyArena;
            let mut allocator_refcell: RefMut<IntAllocator> = arena_ptr.allocator();
            let allocator: &mut IntAllocator = &mut allocator_refcell as &mut IntAllocator;

            let program = node_from_bytes(allocator, program_blob)?;
            let args = node_from_bytes(allocator, args_blob)?;
            (program, args)
        };
        self.run_program_ptr(py, &arena, program, args, max_cost)
    }
}

impl Dialect {
    pub fn run_program_ptr<'p>(
        &self,
        py: Python<'p>,
        arena: &PyCell<PyArena>,
        program: i32,
        args: i32,
        max_cost: Cost,
    ) -> PyResult<(Cost, ArenaObject)> {
        let borrowed_arena = arena.borrow();
        let mut allocator_refcell: RefMut<IntAllocator> = borrowed_arena.allocator();
        let allocator: &mut IntAllocator = &mut allocator_refcell as &mut IntAllocator;

        let drc = DialectRunningContext {
            dialect: self,
            arena: &borrowed_arena,
        };

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
                let r = ArenaObject::new(py, arena, reduction.1);
                Ok((reduction.0, r))
            }
            Err(eval_err) => {
                let node: PyObject = arena
                    .borrow()
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
    arena: &'a PyArena,
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
                self.arena.py_for_native(py, args, allocator),
                args,
                "can't uncache",
            )?;
            let r1 = obj.call1(py, (op, r.to_object(py), max_cost));
            match r1 {
                Err(pyerr) => {
                    let eval_err: PyResult<EvalErr<i32>> =
                        eval_err_for_pyerr(py, &pyerr, self.arena, allocator);
                    let r: EvalErr<i32> =
                        unwrap_or_eval_err(eval_err, args, "unexpected exception")?;
                    Err(r)
                }
                Ok(o) => {
                    let pair: &PyTuple = unwrap_or_eval_err(o.extract(py), args, "expected tuple")?;

                    let i0: u32 =
                        unwrap_or_eval_err(pair.get_item(0).extract(), args, "expected u32")?;

                    let clvm_object: &PyCell<CLVMObject> =
                        unwrap_or_eval_err(pair.get_item(1).extract(), args, "expected node")?;

                    let r = self.arena.native_for_py(py, clvm_object, allocator);
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
        } else {
            self.dialect
                .unknown_op_callback
                .invoke(allocator, o, *argument_list, max_cost)
        }
    }
}

/// turn a `PyErr` into an `EvalErr<P>` if at all possible
/// otherwise, return a `PyErr`
fn eval_err_for_pyerr<'p>(
    py: Python<'p>,
    pyerr: &PyErr,
    arena: &'p PyArena,
    allocator: &mut IntAllocator,
) -> PyResult<EvalErr<i32>> {
    let args: &PyTuple = pyerr.pvalue(py).getattr("args")?.extract()?;
    let arg0: &PyString = args.get_item(0).extract()?;
    let sexp: &PyCell<CLVMObject> = pyerr.pvalue(py).getattr("_sexp")?.extract()?;
    let node: i32 = arena.native_for_py(py, sexp, allocator)?;
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
