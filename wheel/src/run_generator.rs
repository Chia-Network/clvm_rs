use super::adapt_response::eval_err_to_pyresult;
use clvmr::allocator::{Allocator, NodePtr};
use clvmr::chia_dialect::ChiaDialect;
use clvmr::cost::Cost;
use clvmr::gen::conditions::{parse_spends, Spend, SpendBundleConditions};
use clvmr::gen::validation_error::{ErrorCode, ValidationErr};
use clvmr::reduction::{EvalErr, Reduction};
use clvmr::run_program::run_program;
use clvmr::serialize::node_from_bytes;

use pyo3::prelude::*;
use pyo3::types::PyBytes;

#[pyclass(subclass, unsendable)]
#[derive(Clone)]
pub struct PySpend {
    #[pyo3(get)]
    pub coin_id: PyObject,
    #[pyo3(get)]
    pub puzzle_hash: PyObject,
    #[pyo3(get)]
    pub height_relative: Option<u32>,
    #[pyo3(get)]
    pub seconds_relative: u64,
    #[pyo3(get)]
    pub create_coin: Vec<(PyObject, u64, PyObject)>,
    #[pyo3(get)]
    pub agg_sig_me: Vec<(PyObject, PyObject)>,
}

#[pyclass(subclass, unsendable)]
#[derive(Clone)]
pub struct PySpendBundleConditions {
    #[pyo3(get)]
    pub spends: Vec<PySpend>,
    #[pyo3(get)]
    pub reserve_fee: u64,
    #[pyo3(get)]
    // the highest height/time conditions (i.e. most strict)
    pub height_absolute: u32,
    #[pyo3(get)]
    pub seconds_absolute: u64,
    // Unsafe Agg Sig conditions (i.e. not tied to the spend generating it)
    #[pyo3(get)]
    pub agg_sig_unsafe: Vec<(PyObject, PyObject)>,
    #[pyo3(get)]
    pub cost: u64,
}

fn node_to_pybytes(py: Python, a: &Allocator, n: NodePtr) -> PyObject {
    PyBytes::new(py, a.atom(n)).into()
}

fn convert_spend(py: Python, a: &Allocator, spend: Spend) -> PySpend {
    let mut agg_sigs = Vec::<(PyObject, PyObject)>::new();
    for (pk, msg) in spend.agg_sig_me {
        agg_sigs.push((node_to_pybytes(py, a, pk), node_to_pybytes(py, a, msg)));
    }
    let mut create_coin = Vec::<(PyObject, u64, PyObject)>::new();
    for c in spend.create_coin {
        create_coin.push((
            PyBytes::new(py, &c.puzzle_hash).into(),
            c.amount,
            node_to_pybytes(py, a, c.hint),
        ));
    }

    PySpend {
        coin_id: PyBytes::new(py, &*spend.coin_id).into(),
        puzzle_hash: node_to_pybytes(py, a, spend.puzzle_hash),
        height_relative: spend.height_relative,
        seconds_relative: spend.seconds_relative,
        create_coin,
        agg_sig_me: agg_sigs,
    }
}

fn convert_spend_bundle_conds(
    py: Python,
    a: &Allocator,
    sb: SpendBundleConditions,
) -> PySpendBundleConditions {
    let mut spends = Vec::<PySpend>::new();
    for s in sb.spends {
        spends.push(convert_spend(py, a, s));
    }

    let mut agg_sigs = Vec::<(PyObject, PyObject)>::new();
    for (pk, msg) in sb.agg_sig_unsafe {
        agg_sigs.push((node_to_pybytes(py, a, pk), node_to_pybytes(py, a, msg)));
    }

    PySpendBundleConditions {
        spends,
        reserve_fee: sb.reserve_fee,
        height_absolute: sb.height_absolute,
        seconds_absolute: sb.seconds_absolute,
        agg_sig_unsafe: agg_sigs,
        cost: sb.cost,
    }
}

// returns the cost of running the CLVM program along with conditions and the list of
// spends
#[pyfunction]
pub fn run_generator2(
    py: Python,
    program: &[u8],
    args: &[u8],
    max_cost: Cost,
    flags: u32,
) -> PyResult<(Option<u32>, Option<PySpendBundleConditions>)> {
    let mut allocator = Allocator::new();
    let program = node_from_bytes(&mut allocator, program)?;
    let args = node_from_bytes(&mut allocator, args)?;
    let dialect = &ChiaDialect::new(flags);

    let r = py.allow_threads(
        || -> Result<(Option<ErrorCode>, Option<SpendBundleConditions>), EvalErr> {
            let Reduction(cost, node) =
                run_program(&mut allocator, dialect, program, args, max_cost, None)?;
            // we pass in what's left of max_cost here, to fail early in case the
            // cost of a condition brings us over the cost limit
            match parse_spends(&allocator, node, max_cost - cost, flags) {
                Err(ValidationErr(_, c)) => Ok((Some(c), None)),
                Ok(mut spend_bundle_conds) => {
                    // the cost is only the cost of conditions, add the
                    // cost of running the CLVM program here as well
                    spend_bundle_conds.cost += cost;
                    Ok((None, Some(spend_bundle_conds)))
                }
            }
        },
    );

    match r {
        Ok((None, Some(spend_bundle_conds))) => {
            // everything was successful
            Ok((
                None,
                Some(convert_spend_bundle_conds(
                    py,
                    &allocator,
                    spend_bundle_conds,
                )),
            ))
        }
        Ok((error_code, _)) => {
            // a validation error occurred
            Ok((error_code.map(|x| x.into()), None))
        }
        Err(eval_err) => eval_err_to_pyresult(py, eval_err, allocator),
    }
}
