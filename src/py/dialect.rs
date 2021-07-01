use std::cell::RefMut;
use std::collections::HashMap;

use pyo3::prelude::{pyclass, pymethods};

use pyo3::types::{PyString, PyTuple};
use pyo3::{FromPyObject, PyAny, PyObject, PyRef, PyResult, Python, ToPyObject};

use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::reduction::EvalErr;
use crate::reduction::Reduction;
use crate::reduction::Response;
use crate::run_program::{OperatorHandler, PostEval, PreEval};
use crate::serialize::node_from_bytes;

use super::arena::Arena;
use super::error_bridge::{eval_err_for_pyerr, raise_eval_error, unwrap_or_eval_err};
use super::f_table::FLookup;
use super::f_table::OpFn;
use super::native_op::NativeOp;

type MultiOpFn = fn(&mut Allocator, NodePtr, NodePtr, Cost) -> Response;

#[pyclass]
#[derive(Clone)]
pub struct PyMultiOpFn {
    op: MultiOpFn,
}

impl PyMultiOpFn {
    pub fn new(op: MultiOpFn) -> Self {
        Self { op }
    }
}

#[derive(Clone)]
pub enum MultiOpFnE {
    Python(PyObject),
    Rust(MultiOpFn),
}

impl MultiOpFnE {
    pub fn invoke(
        &self,
        allocator: &mut Allocator,
        o: NodePtr,
        args: NodePtr,
        max_cost: Cost,
    ) -> Response {
        match self {
            Self::Python(_o) => {
                todo!()
            }
            Self::Rust(f) => f(allocator, o, args, max_cost),
        }
    }
}

impl<'source> FromPyObject<'source> for MultiOpFnE {
    fn extract(obj: &'source pyo3::PyAny) -> PyResult<Self> {
        let v: PyResult<PyRef<PyMultiOpFn>> = obj.extract();
        if let Ok(v) = v {
            Ok(Self::Rust(v.op))
        } else {
            Ok(Self::Python(obj.into()))
        }
    }
}

#[pyclass]
pub struct Dialect {
    quote_kw: Vec<u8>,
    apply_kw: Vec<u8>,
    u8_lookup: FLookup,
    python_u8_lookup: HashMap<Vec<u8>, PyObject>,
    native_u8_lookup: HashMap<Vec<u8>, OpFn>,
    unknown_op_callback: MultiOpFnE,
    to_python: PyObject,
}

#[pymethods]
impl Dialect {
    #[new]
    pub fn new(
        quote_kw: Vec<u8>,
        apply_kw: Vec<u8>,
        unknown_op_callback: MultiOpFnE,
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
        let arena_cell = Arena::new_cell_obj(py, self.to_python.clone())?;
        let arena: PyRef<Arena> = arena_cell.borrow();

        let program = arena.ptr_for_obj(py, program)?;
        let args = arena.ptr_for_obj(py, args)?;

        let (cost, r) = self.run_program_ptr(py, arena, program, args, max_cost, pre_eval_f)?;
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
        let arena_cell = Arena::new_cell_obj(py, self.to_python.clone())?;
        let arena = arena_cell.borrow();
        let (program, args) = {
            let mut allocator_refcell: RefMut<Allocator> = arena.allocator();
            let allocator: &mut Allocator = &mut allocator_refcell as &mut Allocator;

            let program = node_from_bytes(allocator, program_blob)?;
            let args = node_from_bytes(allocator, args_blob)?;
            (program, args)
        };
        self.run_program_ptr(py, arena, program, args, max_cost, pre_eval)
    }
}

fn pre_eval_callback(
    py: Python,
    arena: &Arena,
    pre_eval_obj: PyObject,
    allocator: &mut Allocator,
    program: NodePtr,
    args: NodePtr,
) -> PyResult<PyObject> {
    // call the python `pre_eval` object and return the python object yielded
    let program_obj = arena.cache.py_for_native(py, program, allocator)?;
    let args_obj = arena.cache.py_for_native(py, args, allocator)?;
    let post_eval_obj = pre_eval_obj
        .call1(py, (program_obj, args_obj))?
        .to_object(py);
    Ok(post_eval_obj)
}

impl Dialect {
    pub fn run_program_ptr<'p>(
        &self,
        py: Python<'p>,
        arena: PyRef<'p, Arena>,
        program: i32,
        args: i32,
        max_cost: Cost,
        pre_eval: &'p PyAny,
    ) -> PyResult<(Cost, &'p PyAny)> {
        let mut allocator_refcell: RefMut<Allocator> = arena.allocator();
        let allocator: &mut Allocator = &mut allocator_refcell as &mut Allocator;

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
                let local_pre_eval: PreEval = Box::new(|allocator, program, args| {
                    if let Ok(post_eval_obj) = pre_eval_callback(
                        py,
                        &arena,
                        pre_eval_obj.clone(),
                        allocator,
                        program,
                        args,
                    ) {
                        let local_arena = &arena;
                        let post_eval: Box<PostEval> =
                            Box::new(move |allocator: &mut Allocator, result_ptr: i32| {
                                if let Ok(r) =
                                    local_arena.cache.py_for_native(py, result_ptr, allocator)
                                {
                                    // invoke the python `PostEval` callback
                                    let _r = post_eval_obj.call1(py, (r.to_object(py),));
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

        let r: Result<Reduction, EvalErr> = crate::run_program::run_program(
            allocator,
            program,
            args,
            &self.quote_kw,
            &self.apply_kw,
            max_cost,
            &drc,
            pre_eval_f,
        );

        match r {
            Ok(reduction) => {
                let r = arena.cache.py_for_native(py, reduction.1, allocator)?;
                Ok((reduction.0, r))
            }
            Err(eval_err) => {
                let node: PyObject = arena
                    .cache
                    .py_for_native(py, eval_err.0, allocator)?
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
    arena: &'a PyRef<'a, Arena>,
}

impl DialectRunningContext<'_> {
    pub fn invoke_py_obj(
        &self,
        obj: &PyObject,
        allocator: &mut Allocator,
        args: NodePtr,
        max_cost: Cost,
    ) -> Response {
        Python::with_gil(|py| {
            let r = unwrap_or_eval_err(
                self.arena.cache.py_for_native(py, args, allocator),
                args,
                "can't uncache",
            )?;
            let r1 = obj.call1(py, (r.to_object(py), max_cost));
            match r1 {
                Err(pyerr) => {
                    let eval_err: PyResult<EvalErr> =
                        eval_err_for_pyerr(py, &pyerr, &self.arena, allocator);
                    let r: EvalErr = unwrap_or_eval_err(eval_err, args, "unexpected exception")?;
                    Err(r)
                }
                Ok(o) => {
                    let pair: &PyTuple = unwrap_or_eval_err(o.extract(py), args, "expected tuple")?;

                    let i0: u32 =
                        unwrap_or_eval_err(pair.get_item(0).extract(), args, "expected u32")?;

                    let clvm_object: &PyAny =
                        unwrap_or_eval_err(pair.get_item(1).extract(), args, "expected node")?;

                    let r = self.arena.cache.native_for_py(py, clvm_object, allocator);
                    let node: i32 = unwrap_or_eval_err(r, args, "can't find in int allocator")?;
                    Ok(Reduction(i0 as Cost, node))
                }
            }
        })
    }
}

impl OperatorHandler for DialectRunningContext<'_> {
    fn op(
        &self,
        allocator: &mut Allocator,
        o: NodePtr,
        argument_list: NodePtr,
        max_cost: Cost,
    ) -> Response {
        let op = &allocator.atom(o);
        if op.len() == 1 {
            if let Some(f) = self.dialect.u8_lookup[op[0] as usize] {
                return f(allocator, argument_list, max_cost);
            }
        }
        let op = op.to_owned();
        if let Some(op_fn) = self.dialect.native_u8_lookup.get(op) {
            op_fn(allocator, argument_list, max_cost)
        } else if let Some(op_fn) = self.dialect.python_u8_lookup.get(op) {
            self.invoke_py_obj(op_fn, allocator, argument_list, max_cost)
        } else {
            self.dialect
                .unknown_op_callback
                .invoke(allocator, o, argument_list, max_cost)
        }
    }
}
