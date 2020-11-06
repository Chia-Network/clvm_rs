use super::native_op_lookup::NativeOpLookup;
use super::node::Node;
use super::operators::{default_operator_lookup, DefaultOperatorLookupT};
use super::pysexp::PySExp;
use super::run_program::run_program;
use super::serialize::{node_from_stream, node_to_stream};
use super::types::{EvalContext, EvalErr, FApply, OperatorHandler, PostEval, PreEval, Reduction};
use pyo3::prelude::*;
use pyo3::types::PyTuple;
use pyo3::wrap_pyfunction;
use pyo3::PyObject;
use std::io::Cursor;
use std::io::{Seek, SeekFrom, Write};

impl From<PyErr> for EvalErr {
    fn from(_err: PyErr) -> Self {
        EvalErr(Node::blob("PyErr"), "bad type from python call".to_string())
    }
}

fn node_from_bytes(b: &[u8]) -> std::io::Result<Node> {
    let mut buffer = Cursor::new(Vec::new());
    buffer.write_all(&b)?;
    buffer.seek(SeekFrom::Start(0))?;
    node_from_stream(&mut buffer)
}

fn node_to_bytes(node: &Node) -> std::io::Result<Vec<u8>> {
    let mut buffer = Cursor::new(Vec::new());

    node_to_stream(node, &mut buffer)?;
    let vec = buffer.into_inner();
    Ok(vec)
}

struct PyWrapApply {
    apply_f: PyObject,
}

impl PyWrapApply {
    fn inner_apply(
        &self,
        _eval_context: &EvalContext,
        operator: &Node,
        args: &Node,
    ) -> Result<Reduction, EvalErr> {
        let gil = Python::acquire_gil();
        let py = gil.python();
        let byte_vec: Vec<u8> = self
            .apply_f
            .call1(py, (node_to_bytes(&operator)?, node_to_bytes(&args)?))?
            .extract(py)?;
        let bytes: &[u8] = &byte_vec;
        Ok(Reduction(node_from_bytes(bytes)?, 1000))
    }
}

impl FApply for PyWrapApply {
    fn apply(
        &self,
        eval_context: &EvalContext,
        operator: &Node,
        args: &Node,
    ) -> Option<Result<Reduction, EvalErr>> {
        Some(self.inner_apply(eval_context, operator, args))
    }
}

//let env = node_from_bytes(env_u8.as_bytes())?;
//let f_table = make_f_lookup();

//let py_apply: Box<dyn FApply> = Box::new(PyWrapApply {apply_f});

/*
let pre_eval: PreEval = {
    if py_pre_eval.is_none(py) {
        None
    } else {
        Some(Box::new(
            move |sexp, args, current_cost, max_cost| -> Result<PostEval, EvalErr> {
                let py_post_eval: PyObject = py_pre_eval
                    .call1(
                        py,
                        (
                            node_to_bytes(&sexp)?,
                            node_to_bytes(&args)?,
                            current_cost,
                            max_cost,
                        ),
                    )?
                    .extract(py)?;
                Ok(wrap_py_post_eval(py, py_post_eval))
            },
        ))
    }
};
let pre_eval = None;
let r = run_program(
    &sexp, &env, 0, 100_000, &f_table, py_apply, pre_eval, op_quote, op_args,
);
match r {
    Ok(Reduction(node, cycles)) => Ok(("".into(), node_to_bytes(&node)?, cycles)),
    Err(EvalErr(node, err)) => Ok((err, node_to_bytes(&node)?, 0)),
}
*/

#[pyclass(subclass, unsendable)]
pub struct PyOperatorLookup {
    pub val: DefaultOperatorLookupT,
}

/*
impl OperatorHandler for PyOperatorLookup {
    fn f_for_operator(&self, op: &[u8]) -> Option<&Box<dyn OperatorFT>> {
        self.val.f_for_operator(op)
    }
}
*/

fn op_handler_for_py(obj: &PyAny) -> OperatorHandler {
    let local_obj: PyObject = obj.into();
    Box::new(move |op: &[u8], argument_list: &Node| {
        let pysexp: PySExp = argument_list.clone().into();
        let r1 = Python::with_gil(|py| local_obj.call1(py, (op, pysexp)));
        match r1 {
            Err(_) => argument_list.err("fooooooooo"),
            Ok(o) => Python::with_gil(|py| {
                let pair: &PyTuple = o.extract(py).unwrap();
                let i0: u32 = pair.get_item(0).extract()?;
                let i1: PyRef<PySExp> = pair.get_item(1).extract()?;
                let n = i1.node.clone();
                let r: Reduction = Reduction(n, i0);
                Ok(r)
            }),
        }
    })
}

/*
#[pyfunction]
fn operator_lookup() -> PyOperatorLookup {
    let op_lookup: PyOperatorLookup = PyOperatorLookup {
        val: default_operator_lookup(),
    };
    op_lookup
}
*/

impl IntoPy<PyOperatorLookup> for DefaultOperatorLookupT {
    fn into_py(self, _py: Python) -> PyOperatorLookup {
        PyOperatorLookup { val: self }
    }
}

struct PyPostEval {
    pub obj: PyObject,
}

impl PostEval for PyPostEval {
    fn note_result(&self, result: Option<&Node>) {
        Python::with_gil(|py| {
            if let Some(node) = result {
                let py_sexp: PySExp = node.clone().into();
                let _r: PyResult<PyObject> = self.obj.call1(py, (py_sexp,));
            }
        });
    }
}

fn post_eval_for_pyobject(obj: PyObject) -> Option<Box<dyn PostEval>> {
    let mut py_post_eval: Option<Box<dyn PostEval>> =
        Some(Box::new(PyPostEval { obj: obj.clone() }));

    if Python::with_gil(|py| obj.is_none(py)) {
        py_post_eval = None;
    }
    py_post_eval
}

struct PyPreEval<'a> {
    pub py: Python<'a>,
    pub obj: PyObject,
}

impl PreEval for PyPreEval<'_> {
    fn note_eval_state(
        &self,
        program: &Node,
        args: &Node,
    ) -> Result<Option<Box<dyn PostEval>>, EvalErr> {
        let prog_sexp: PySExp = program.clone().into_py(self.py);
        let args_sexp: PySExp = args.clone().into_py(self.py);
        let r: PyResult<PyObject> = self.obj.call1(self.py, (prog_sexp, args_sexp));
        match r {
            Ok(py_post_eval) => {
                let f = post_eval_for_pyobject(py_post_eval);
                Ok(f)
            }
            Err(ref err) => program.err(&err.to_string()),
        }
    }
}

fn returns_closure(
    native_op_lookup: NativeOpLookup,
) -> Box<dyn Fn(&[u8], &Node) -> Result<Reduction, EvalErr>> {
    let f: OperatorHandler = Box::new(move |op, args| native_op_lookup.operator_handler(op, args));
    f
}

#[pyfunction]
fn py_run_program(
    py: Python,
    program: &PySExp,
    args: &PySExp,
    quote_kw: u8,
    max_cost: u32,
    op_lookup: NativeOpLookup,
    pre_eval: PyObject,
) -> PyResult<(String, PySExp, u32)> {
    let py_pre_eval_inner: PyPreEval = PyPreEval {
        py,
        obj: pre_eval.clone(),
    };

    let mut py_pre_eval: Option<&dyn PreEval> = Some(&py_pre_eval_inner);

    if pre_eval.is_none(py) {
        py_pre_eval = None;
    }

    //let new_operator_for_opcode = op_handler_for_py(op_lookup);

    let native_op_lookup: NativeOpLookup = op_lookup;

    let f = returns_closure(native_op_lookup);

    let r: Result<Reduction, EvalErr> = run_program(
        &program.node,
        &args.node,
        quote_kw,
        max_cost,
        &f,
        py_pre_eval,
    );
    match r {
        Ok(reduction) => Ok(("worked".into(), reduction.0.into(), reduction.1)),
        Err(eval_err) => Ok((eval_err.1, eval_err.0.into(), 1)),
    }
    // TODO: recast to same type as `program`
}

/// This module is a python module implemented in Rust.
#[pymodule]
fn clvm_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(py_run_program, m)?)
        .unwrap();
    //   m.add_function(wrap_pyfunction!(operator_lookup, m)?)
    //    .unwrap();
    m.add_class::<PySExp>()?;
    m.add_class::<NativeOpLookup>()?;
    Ok(())
}
