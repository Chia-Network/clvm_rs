use js_sys::Array;
use std::rc::Rc;
use wasm_bindgen::prelude::*;

use crate::lazy_node::LazyNode;
use clvmr::allocator::Allocator;
use clvmr::ALLOW_BACKREFS;
use clvmr::chia_dialect::ChiaDialect;
use clvmr::chia_dialect::NO_UNKNOWN_OPS as _no_unknown_ops;
use clvmr::cost::Cost;
use clvmr::run_program::run_program;
use clvmr::serde::{node_from_bytes, node_from_bytes_backrefs, node_to_bytes};

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
    pub fn no_unknown_ops() -> u32 {
        _no_unknown_ops
    }

    #[wasm_bindgen]
    pub fn allow_backrefs() -> u32 {
        ALLOW_BACKREFS
    }
}

#[wasm_bindgen]
pub fn run_clvm(program: &[u8], args: &[u8], flag: u32) -> Vec<u8> {
    let max_cost: Cost = 1_000_000_000_000_000;

    let mut allocator = Allocator::new();
    let deserializer = if (flag & ALLOW_BACKREFS) != 0 {
        node_from_bytes_backrefs
    } else {
        node_from_bytes
    };
    let program = deserializer(&mut allocator, program).unwrap();
    let args = deserializer(&mut allocator, args).unwrap();
    let dialect = ChiaDialect::new(flag);

    let r = run_program(
        &mut allocator,
        &dialect,
        program,
        args,
        max_cost,
    );
    match r {
        Ok(reduction) => node_to_bytes(&allocator, reduction.1).unwrap(),
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
    let deserializer = if (flag & ALLOW_BACKREFS) != 0 {
        node_from_bytes_backrefs
    } else {
        node_from_bytes
    };
    let program = deserializer(&mut allocator, program).unwrap();
    let args = deserializer(&mut allocator, args).unwrap();
    let dialect = ChiaDialect::new(flag);

    let r = run_program(&mut allocator, &dialect, program, args, max_cost);
    match r {
        Ok(reduction) => {
            let cost = JsValue::from(reduction.0);
            let node = LazyNode::new(Rc::new(allocator), reduction.1);
            let val = JsValue::from(node);

            let tuple = Array::new_with_length(2);
            tuple.set(0, cost);
            tuple.set(1, val);
            Ok(tuple)
        }
        Err(_eval_err) => Err(format!("{:?}", _eval_err)),
    }
}
