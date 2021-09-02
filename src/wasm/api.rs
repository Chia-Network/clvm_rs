use wasm_bindgen::prelude::*;

use crate::allocator::{Allocator, NodePtr};
use crate::chia_dialect::ChiaDialect;
use crate::cost::Cost;
use crate::err_utils::err;
use crate::more_ops::op_unknown;
use crate::node::Node;
use crate::reduction::Response;
use crate::run_program::run_program;
use crate::serialize::{node_from_bytes, node_to_bytes};

// When the `wee_alloc` feature is enabled, use `wee_alloc` as the global
// allocator.
#[cfg(feature = "wee_alloc")]
#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

// NOTE: This is just a proof of concept.
// Ideally, the wasm api will have more features, like the
// python api. For now, this is just a sanity check that something
// works at all.
//
// TODO: replace the below with something more robust and feature-filled

#[wasm_bindgen]
pub fn run_clvm(program: &[u8], args: &[u8]) -> Vec<u8> {
    let max_cost: Cost = 1_000_000_000_000_000;

    let strict: bool = false;
    let mut allocator = Allocator::new();
    let program = node_from_bytes(&mut allocator, program).unwrap();
    let args = node_from_bytes(&mut allocator, args).unwrap();
    let r = run_program(
        &mut allocator,
        &ChiaDialect::new(strict),
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
