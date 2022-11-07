use std::rc::Rc;
use wasm_bindgen::prelude::*;
use js_sys::{Array};

use crate::lazy_node::LazyNode;
use clvmr::allocator::Allocator;
use clvmr::chia_dialect::ChiaDialect;
use clvmr::chia_dialect::NO_NEG_DIV as _no_neg_div;
use clvmr::chia_dialect::NO_UNKNOWN_OPS as _no_unknown_ops;
use clvmr::cost::Cost;
use clvmr::node::Node;
use clvmr::run_program::run_program;
use clvmr::serde::{node_from_bytes, node_to_bytes, serialized_length_from_bytes};

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

#[wasm_bindgen]
pub struct Flag;

#[wasm_bindgen]
impl Flag {
    #[wasm_bindgen]
    pub fn no_neg_div() -> u32 { _no_neg_div }

    #[wasm_bindgen]
    pub fn no_unknown_ops() -> u32 { _no_unknown_ops }
}

#[wasm_bindgen]
pub fn serialized_length(program: &[u8]) -> Result<u64, String> {
    match serialized_length_from_bytes(program) {
        Ok(length) => Ok(length),
        Err(err) => Err(err.to_string()),
    }
}

#[wasm_bindgen]
pub fn run_clvm(program: &[u8], args: &[u8]) -> Vec<u8> {
    let max_cost: Cost = 1_000_000_000_000_000;

    let mut allocator = Allocator::new();
    let program = node_from_bytes(&mut allocator, program).unwrap();
    let args = node_from_bytes(&mut allocator, args).unwrap();

    let r = run_program(
        &mut allocator,
        &ChiaDialect::new(0),
        program,
        args,
        max_cost,
        None,
    );
    match r {
        Ok(reduction) => node_to_bytes(&Node::new(&allocator, reduction.1)).unwrap(),
        Err(_eval_err) => format!("{:?}", _eval_err).into(),
    }
}

#[wasm_bindgen]
pub fn run_chia_program(
    program: &[u8],
    args: &[u8],
    max_cost: Cost, // Expecting `BigInt` to be passed from JavaScript world
    flag: u32,
) -> Result<Array, String> {
    let mut allocator = Allocator::new();
    let program = node_from_bytes(&mut allocator, program).unwrap();
    let args = node_from_bytes(&mut allocator, args).unwrap();
    let dialect = ChiaDialect::new(flag);

    let r = run_program(
        &mut allocator,
        &dialect,
        program,
        args,
        max_cost,
        None,
    );
    match r {
        Ok(reduction) => {
            let cost = JsValue::from(reduction.0);
            let node = LazyNode::new(Rc::new(allocator), reduction.1);
            let val = JsValue::from(node);

            let tuple = Array::new_with_length(2);
            tuple.set(0, cost);
            tuple.set(1, val);
            Ok(tuple)
        },
        Err(_eval_err) => Err(format!("{:?}", _eval_err).into()),
    }
}
