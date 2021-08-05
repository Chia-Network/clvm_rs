use clvm_rs::allocator::{Allocator, NodePtr};
use clvm_rs::cost::Cost;
use clvm_rs::reduction::{EvalErr, Reduction};
use clvm_rs::run_program::{run_program, STRICT_MODE};
use clvm_rs::serialize::node_from_bytes;

use clvm_rs::f_table::f_lookup_for_hashmap;

use std::collections::HashMap;
use std::rc::Rc;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use super::gen::conditions::{parse_spends, Condition, SpendConditionSummary};
use super::gen::opcodes::{
    ConditionOpcode, AGG_SIG_ME, AGG_SIG_UNSAFE, ASSERT_HEIGHT_ABSOLUTE, ASSERT_HEIGHT_RELATIVE,
    ASSERT_SECONDS_ABSOLUTE, ASSERT_SECONDS_RELATIVE, CREATE_COIN, RESERVE_FEE,
};
use super::gen::validation_error::{ErrorCode, ValidationErr};
use super::int_to_bytes::u64_to_bytes;
use super::lazy_node::LazyNode;
use super::run_program::OperatorHandlerWithMode;

fn node_to_pybytes(py: Python, a: &Allocator, n: NodePtr) -> PyObject {
    PyBytes::new(py, a.atom(n)).into()
}

fn int_to_pybytes(py: Python, n: u64) -> PyObject {
    let buf = u64_to_bytes(n);
    PyBytes::new(py, &buf).into()
}

#[pyclass(subclass, unsendable)]
#[derive(Clone)]
pub struct PyConditionWithArgs {
    #[pyo3(get)]
    pub opcode: ConditionOpcode,
    #[pyo3(get)]
    pub vars: Vec<PyObject>,
}

#[pyclass(subclass, unsendable)]
pub struct PySpendConditionSummary {
    #[pyo3(get)]
    pub coin_name: PyObject,
    #[pyo3(get)]
    pub puzzle_hash: PyObject,
    #[pyo3(get)]
    pub conditions: Vec<(ConditionOpcode, Vec<PyConditionWithArgs>)>,
}

fn convert_condition(py: Python, a: &Allocator, c: Condition) -> PyConditionWithArgs {
    let (vars, opcode) = match c {
        Condition::AggSigUnsafe(pubkey, msg) => (
            vec![node_to_pybytes(py, a, pubkey), node_to_pybytes(py, a, msg)],
            AGG_SIG_UNSAFE,
        ),
        Condition::AggSigMe(pubkey, msg) => (
            vec![node_to_pybytes(py, a, pubkey), node_to_pybytes(py, a, msg)],
            AGG_SIG_ME,
        ),
        _ => {
            panic!("unexpected condition");
        }
    };
    PyConditionWithArgs { opcode, vars }
}

fn make_condition(py: Python, op: ConditionOpcode, val: u64) -> Vec<PyConditionWithArgs> {
    vec![PyConditionWithArgs {
        opcode: op,
        vars: vec![int_to_pybytes(py, val)],
    }]
}

fn convert_spend(
    py: Python,
    a: &Allocator,
    spend_cond: SpendConditionSummary,
) -> PySpendConditionSummary {
    let mut ordered = HashMap::<ConditionOpcode, Vec<PyConditionWithArgs>>::new();
    for c in spend_cond.agg_sigs {
        let op = match c {
            Condition::AggSigUnsafe(_, _) => AGG_SIG_UNSAFE,
            Condition::AggSigMe(_, _) => AGG_SIG_ME,
            _ => {
                panic!("unexpected condition");
            }
        };
        match ordered.get_mut(&op) {
            Some(set) => {
                set.push(convert_condition(py, a, c));
            }
            None => {
                ordered.insert(op, vec![convert_condition(py, a, c)]);
            }
        };
    }

    let mut new_coins = Vec::<PyConditionWithArgs>::new();
    for (ph, amount) in spend_cond.create_coin {
        new_coins.push(PyConditionWithArgs {
            opcode: CREATE_COIN,
            vars: vec![PyBytes::new(py, &ph).into(), int_to_pybytes(py, amount)],
        });
    }
    if !new_coins.is_empty() {
        ordered.insert(CREATE_COIN, new_coins);
    }

    if spend_cond.reserve_fee > 0 {
        ordered.insert(
            RESERVE_FEE,
            make_condition(py, RESERVE_FEE, spend_cond.reserve_fee),
        );
    }

    if let Some(h) = spend_cond.height_relative {
        ordered.insert(
            ASSERT_HEIGHT_RELATIVE,
            make_condition(py, ASSERT_HEIGHT_RELATIVE, h as u64),
        );
    }

    if spend_cond.height_absolute > 0 {
        ordered.insert(
            ASSERT_HEIGHT_ABSOLUTE,
            make_condition(
                py,
                ASSERT_HEIGHT_ABSOLUTE,
                spend_cond.height_absolute as u64,
            ),
        );
    }

    if spend_cond.seconds_relative > 0 {
        ordered.insert(
            ASSERT_SECONDS_RELATIVE,
            make_condition(
                py,
                ASSERT_SECONDS_RELATIVE,
                spend_cond.seconds_relative as u64,
            ),
        );
    }

    if spend_cond.seconds_absolute > 0 {
        ordered.insert(
            ASSERT_SECONDS_ABSOLUTE,
            make_condition(
                py,
                ASSERT_SECONDS_ABSOLUTE,
                spend_cond.seconds_absolute as u64,
            ),
        );
    }

    let mut conditions = Vec::<(ConditionOpcode, Vec<PyConditionWithArgs>)>::new();
    for (k, v) in ordered {
        conditions.push((k, v));
    }

    PySpendConditionSummary {
        coin_name: PyBytes::new(py, &*spend_cond.coin_id).into(),
        puzzle_hash: node_to_pybytes(py, a, spend_cond.puzzle_hash),
        conditions,
    }
}

impl IntoPy<PyObject> for ErrorCode {
    fn into_py(self, py: Python) -> PyObject {
        let ret = match self {
            ErrorCode::NegativeAmount => 124,
            ErrorCode::InvalidPuzzleHash => 10,
            ErrorCode::InvalidPubkey => 10,
            ErrorCode::InvalidMessage => 10,
            ErrorCode::InvalidParentId => 10,
            ErrorCode::InvalidConditionOpcode => 10,
            ErrorCode::InvalidCoinAnnouncement => 10,
            ErrorCode::InvalidPuzzleAnnouncement => 10,
            ErrorCode::InvalidCondition => 10,
            ErrorCode::InvalidCoinAmount => 10,
            ErrorCode::AssertHeightAbsolute => 14,
            ErrorCode::AssertHeightRelative => 13,
            ErrorCode::AssertSecondsAbsolute => 15,
            ErrorCode::AssertSecondsRelative => 105,
            ErrorCode::AssertMyAmountFailed => 116,
            ErrorCode::AssertMyPuzzlehashFailed => 115,
            ErrorCode::AssertMyParentIdFailed => 114,
            ErrorCode::AssertMyCoinIdFailed => 11,
            ErrorCode::AssertPuzzleAnnouncementFailed => 12,
            ErrorCode::AssertCoinAnnouncementFailed => 12,
            ErrorCode::ReserveFeeConditionFailed => 48,
            ErrorCode::DuplicateOutput => 4,
            ErrorCode::DoubleSpend => 5,
            ErrorCode::CostExceeded => 23,
        };
        ret.to_object(py)
    }
}

// returns the cost of running the CLVM program along with the list of NPCs for
// the generator/spend bundle. Each SpendConditionSummary is a coin spend along with its
// conditions
#[allow(clippy::too_many_arguments)]
#[pyfunction]
pub fn run_generator(
    py: Python,
    program: &[u8],
    args: &[u8],
    quote_kw: u8,
    apply_kw: u8,
    opcode_lookup_by_name: HashMap<String, Vec<u8>>,
    max_cost: Cost,
    flags: u32,
) -> PyResult<(Option<ErrorCode>, Vec<PySpendConditionSummary>, Cost)> {
    let mut allocator = Allocator::new();
    let f_lookup = f_lookup_for_hashmap(opcode_lookup_by_name);
    let strict: bool = (flags & STRICT_MODE) != 0;
    let f = OperatorHandlerWithMode::new(f_lookup, strict);
    let program = node_from_bytes(&mut allocator, program)?;
    let args = node_from_bytes(&mut allocator, args)?;

    let r = py.allow_threads(
        || -> Result<(Option<ErrorCode>, Cost, Vec<SpendConditionSummary>), EvalErr> {
            let Reduction(cost, node) = run_program(
                &mut allocator,
                program,
                args,
                &[quote_kw],
                &[apply_kw],
                max_cost,
                &f,
                None,
            )?;
            // we pass in what's left of max_cost here, to fail early in case the
            // cost of a condition brings us over the cost limit
            match parse_spends(&allocator, node, max_cost - cost, flags) {
                Err(ValidationErr(_, c)) => {
                    Ok((Some(c), 0_u64, Vec::<SpendConditionSummary>::new()))
                }
                Ok(spend_list) => Ok((None, cost, spend_list)),
            }
        },
    );

    let mut ret = Vec::<PySpendConditionSummary>::new();
    match r {
        Ok((None, cost, spend_list)) => {
            // everything was successful
            for spend_cond in spend_list {
                ret.push(convert_spend(py, &allocator, spend_cond));
            }
            Ok((None, ret, cost))
        }
        Ok((error_code, _, _)) => {
            // a validation error occurred
            Ok((error_code, ret, 0))
        }
        Err(eval_err) => {
            let node = LazyNode::new(Rc::new(allocator), eval_err.0);
            let msg = eval_err.1;
            let ctx: &PyDict = PyDict::new(py);
            ctx.set_item("msg", msg)?;
            ctx.set_item("node", node)?;
            Err(py
                .run(
                    "
from clvm.EvalError import EvalError
raise EvalError(msg, node)",
                    None,
                    Some(ctx),
                )
                .unwrap_err())
        }
    }
}
