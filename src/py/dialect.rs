use std::cell::RefMut;
use std::collections::HashMap;
use std::sync::Arc;

use pyo3::prelude::{pyclass, pymethods};

use pyo3::types::{PyString, PyTuple};
use pyo3::{FromPyObject, PyAny, PyCell, PyObject, PyRef, PyResult, Python, ToPyObject};

use crate::allocator::Allocator;
use crate::cost::Cost;
use crate::int_allocator::IntAllocator;
use crate::reduction::EvalErr;
use crate::reduction::Reduction;
use crate::reduction::Response;
use crate::run_program::{OperatorHandler, PostEval, PreEval};
use crate::serialize::node_from_bytes;

use super::error_bridge::{eval_err_for_pyerr, raise_eval_error, unwrap_or_eval_err};
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

#[pyclass]
pub struct Dialect {
    quote_kw: Vec<u8>,
    apply_kw: Vec<u8>,
    u8_lookup: FLookup<IntAllocator>,
    python_u8_lookup: HashMap<Vec<u8>, PyObject>,
    native_u8_lookup: HashMap<Vec<u8>, OpFn<IntAllocator>>,
    unknown_op_callback: MultiOpFnE<IntAllocator>,
    to_python: PyObject,
}

#[pymethods]
impl Dialect {
    #[new]
    pub fn new(
        quote_kw: Vec<u8>,
        apply_kw: Vec<u8>,
        unknown_op_callback: MultiOpFnE<IntAllocator>,
        to_python: PyObject,
    ) -> PyResult<Self> {
        let u8_lookup = [None; 256];
        let python_u8_lookup = HashMap::new();
        let native_u8_lookup = HashMap::new();
        Ok(Self {
            quote_kw,
            apply_kw,
            u8_lookup,
            python_u8_lookup,
            native_u8_lookup,
            unknown_op_callback,
            to_python,
        })
    }

    pub fn update(&mut self, py: Python, d: HashMap<Vec<u8>, PyObject>) -> PyResult<()> {
        for (op, fn_obj) in d.iter() {
            let r: PyResult<PyRef<NativeOp>> = fn_obj.extract(py);
            if let Ok(native_op) = r {
                if op.len() == 1 {
                    let index = op[0] as usize;
                    self.u8_lookup[index] = Some(native_op.op);
                } else {
                    self.native_u8_lookup.insert(op.to_owned(), native_op.op);
                }
            } else {
                self.python_u8_lookup.insert(op.to_owned(), fn_obj.clone());
            }
        }
        Ok(())
    }

    pub fn run_program<'p>(
        &self,
        py: Python<'p>,
        program: &PyAny,
        args: &PyAny,
        max_cost: Cost,
        pre_eval_f: &PyAny,
    ) -> PyResult<(Cost, PyObject)> {
        let arena = PyArena::new_cell_obj(py, self.to_python.clone())?;
        let arena_ptr: &PyArena = &arena.borrow() as &PyArena;

        let program = arena_ptr.ptr_for_obj(py, program)?;
        let args = arena_ptr.ptr_for_obj(py, args)?;

        let (cost, r) = self.run_program_ptr(py, &arena, program, args, max_cost, pre_eval_f)?;
        Ok((cost, r.to_object(py)))
    }

    pub fn deserialize_and_run_program<'p>(
        &self,
        py: Python<'p>,
        program_blob: &[u8],
        args_blob: &[u8],
        max_cost: Cost,
        pre_eval: &'p PyAny,
    ) -> PyResult<(Cost, &'p PyAny)> {
        let arena = PyArena::new_cell_obj(py, self.to_python.clone())?;
        let (program, args) = {
            let arena_ptr: &PyArena = &arena.borrow() as &PyArena;
            let mut allocator_refcell: RefMut<IntAllocator> = arena_ptr.allocator();
            let allocator: &mut IntAllocator = &mut allocator_refcell as &mut IntAllocator;

            let program = node_from_bytes(allocator, program_blob)?;
            let args = node_from_bytes(allocator, args_blob)?;
            (program, args)
        };
        self.run_program_ptr(py, &arena, program, args, max_cost, pre_eval)
    }
}

fn pre_eval_callback(
    py: Python,
    arena: &PyArena,
    pre_eval_obj: PyObject,
    allocator: &mut IntAllocator,
    program: &i32,
    args: &i32,
) -> PyResult<PyObject> {
    // call the python `pre_eval` object and return the python object yielded
    let program_obj = arena.py_for_native(py, program, allocator)?;
    let args_obj = arena.py_for_native(py, args, allocator)?;
    let post_eval_obj = pre_eval_obj
        .call1(py, (program_obj, args_obj))?
        .to_object(py);
    Ok(post_eval_obj)
}

impl Dialect {
    pub fn run_program_ptr<'p>(
        &self,
        py: Python<'p>,
        arena: &'p PyCell<PyArena>,
        program: i32,
        args: i32,
        max_cost: Cost,
        pre_eval: &'p PyAny,
    ) -> PyResult<(Cost, &'p PyAny)> {
        let borrowed_arena: Arc<PyRef<PyArena>> = Arc::new(arena.borrow());
        let mut allocator_refcell: RefMut<IntAllocator> = borrowed_arena.allocator();
        let allocator: &mut IntAllocator = &mut allocator_refcell as &mut IntAllocator;

        let drc = DialectRunningContext {
            dialect: self,
            arena: &arena,
        };

        // we convert `pre_eval` from a python object to a `PreEval`
        // this should be factored out to a standalone function, but
        // lifetimes make it tough!
        let pre_eval_obj = pre_eval.to_object(py);
        let pre_eval_f = {
            if pre_eval.is_none() {
                None
            } else {
                let local_pre_eval: PreEval<IntAllocator> = Box::new(|allocator, program, args| {
                    if let Ok(post_eval_obj) = pre_eval_callback(
                        py,
                        &borrowed_arena,
                        pre_eval_obj.clone(),
                        allocator,
                        program,
                        args,
                    ) {
                        let local_borrowed = borrowed_arena.clone();
                        let post_eval: Box<PostEval<IntAllocator>> =
                            Box::new(move |allocator: &mut IntAllocator, result_ptr: &i32| {
                                if let Ok(r) =
                                    local_borrowed.py_for_native(py, &result_ptr, allocator)
                                {
                                    // invoke the python `PostEval` callback
                                    let _r = post_eval_obj.call1(py, (r,));
                                }
                            });
                        Ok(Some(post_eval))
                    } else {
                        Ok(None)
                    }
                });
                Some(local_pre_eval)
            }
        };

        let r: Result<Reduction<i32>, EvalErr<i32>> = crate::run_program::run_program(
            allocator,
            &program,
            &args,
            &self.quote_kw,
            &self.apply_kw,
            max_cost,
            &drc,
            pre_eval_f,
        );

        match r {
            Ok(reduction) => {
                let r = borrowed_arena.py_for_native(py, &reduction.1, allocator)?;
                Ok((reduction.0, r))
            }
            Err(eval_err) => {
                let node: PyObject = borrowed_arena
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
    arena: &'a PyCell<PyArena>,
}

impl DialectRunningContext<'_> {
    pub fn invoke_py_obj(
        &self,
        obj: &PyObject,
        allocator: &mut IntAllocator,
        args: &<IntAllocator as Allocator>::Ptr,
        max_cost: Cost,
    ) -> Response<<IntAllocator as Allocator>::Ptr> {
        Python::with_gil(|py| {
            let r = unwrap_or_eval_err(
                PyArena::py_for_native(&self.arena.borrow(), py, args, allocator),
                args,
                "can't uncache",
            )?;
            let r1 = obj.call1(py, (r.to_object(py), max_cost));
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

                    let clvm_object: &PyAny =
                        unwrap_or_eval_err(pair.get_item(1).extract(), args, "expected node")?;

                    let r = PyArena::native_for_py(self.arena, py, clvm_object, allocator);
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
            self.invoke_py_obj(op_fn, allocator, argument_list, max_cost)
        } else {
            self.dialect
                .unknown_op_callback
                .invoke(allocator, o, *argument_list, max_cost)
        }
    }
}
