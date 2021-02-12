use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};
use pyo3::PyClass;
use pyo3::PyObject;

use super::arc_allocator::ArcAllocator;
use super::native_op_lookup::GenericNativeOpLookup;
use super::py_node::PyNode;
use super::to_py_node::ToPyNode;

use crate::allocator::Allocator;
use crate::node::Node;
use crate::reduction::{EvalErr, Reduction};
use crate::run_program::{run_program, PostEval, PreEval};
use crate::serialize::{node_from_bytes, node_to_bytes};

impl ToPyNode<PyNode> for ArcAllocator {
    fn to_pynode(&self, ptr: &Self::Ptr) -> PyNode {
        PyNode::new(ptr.clone())
    }
}

fn note_result<T>(obj: &PyObject, result: Option<&T>)
where
    T: ToPyObject,
{
    Python::with_gil(|py| {
        if let Some(node) = result {
            let node: PyObject = node.to_object(py);
            let _r: PyResult<PyObject> = obj.call1(py, (node,));
        }
    });
}

fn post_eval_for_pyobject<A: Allocator>(py: Python, obj: PyObject) -> Option<Box<PostEval<A>>>
where
    A::Ptr: ToPyObject,
{
    let py_post_eval: Option<Box<PostEval<A>>> = if obj.is_none(py) {
        None
    } else {
        Some(Box::new(move |result: Option<&A::Ptr>| {
            note_result(&obj, result)
        }))
    };

    py_post_eval
}

#[allow(clippy::too_many_arguments)]
pub fn _py_run_program<'p, 'a, 'n, A, N>(
    py: Python<'p>,
    allocator: &'a A,
    program: &'n N,
    args: &'n N,
    quote_kw: u8,
    apply_kw: u8,
    max_cost: u32,
    op_lookup: Box<GenericNativeOpLookup<A, N>>,
    pre_eval: PyObject,
) -> PyResult<(u32, N)>
where
    A: 'static + Allocator + ToPyNode<N>,
    N: 'static + PyClass + IntoPy<PyObject> + Clone,
    <A as Allocator>::Ptr: IntoPy<PyObject> + From<&'n N> + From<N> + ToPyObject,
{
    let py_pre_eval_t: Option<PreEval<A>> = if pre_eval.is_none(py) {
        None
    } else {
        Some(Box::new(move |allocator, program, args| {
            Python::with_gil(|py| {
                let program_clone: N = allocator.to_pynode(program);
                let args: N = allocator.to_pynode(args);
                let r: PyResult<PyObject> = pre_eval.call1(py, (program_clone, args));
                match r {
                    Ok(py_post_eval) => Ok(post_eval_for_pyobject::<A>(py, py_post_eval)),
                    Err(ref err) => {
                        let program: Node<A> = Node::new(allocator, program.clone());
                        program.err(&err.to_string())
                    }
                }
            })
        }))
    };

    let r: Result<Reduction<<A as Allocator>::Ptr>, EvalErr<<A as Allocator>::Ptr>> = run_program(
        allocator,
        &program.into(),
        &args.into(),
        quote_kw,
        apply_kw,
        max_cost,
        op_lookup,
        py_pre_eval_t,
    );
    match r {
        Ok(reduction) => Ok((reduction.0, allocator.to_pynode(&reduction.1))),
        Err(eval_err) => {
            let node: PyObject = eval_err.0.to_object(py);
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

pub fn _serialize_from_bytes<A: Allocator, N: PyClass>(allocator: &A, blob: &[u8]) -> N
where
    A: ToPyNode<N>,
{
    allocator.to_pynode(&node_from_bytes(allocator, blob).unwrap())
}

pub fn _serialize_to_bytes<A: Allocator, N>(
    allocator: &A,
    py: Python,
    sexp: &PyAny,
) -> PyResult<PyObject>
where
    N: PyClass + Clone,
    <A as Allocator>::Ptr: From<N>,
{
    let py_node: N = sexp.extract()?;
    let node_t: Node<A> = Node::new(allocator, py_node.into());
    let blob = node_to_bytes(&node_t).unwrap();
    let pybytes = PyBytes::new(py, &blob);
    Ok(pybytes.to_object(py))
}
