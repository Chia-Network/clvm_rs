use std::convert::TryInto;
use wasm_bindgen::prelude::*;

use clvmr::allocator::Allocator;
use clvmr::chia_dialect::ChiaDialect;
use clvmr::chia_dialect::NO_NEG_DIV as _no_neg_div;
use clvmr::chia_dialect::NO_UNKNOWN_OPS as _no_unknown_ops;
use clvmr::cost::Cost;
use clvmr::node::Node;
use clvmr::run_program::run_program;
use clvmr::serialize::{node_from_bytes, node_to_bytes, serialized_length_from_bytes};

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
pub fn serialized_length(program: &[u8]) -> u64
{
    serialized_length_from_bytes(program).unwrap()
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
/**
 * Return serialized result of clvm program with cost
 *
 * cost will be available at the first 8 bytes of returned Vec<u8>(=Uint8Array).
 * bytes of node will be available from 8th byte offset of returned Vec<u8>.
 *
 * @example
 * const result = run_chia_program(...); // Uint8Array
 * const cost = vec_u8_to_u64(result.subarray(0, 8)) // BigInt;
 * const serialized_node = result.subarray(8); // Uint8Array
 */
pub fn run_chia_program(
    program: &[u8],
    args: &[u8],
    max_cost: Cost, // Expecting `BigInt` to be passed from JavaScript world
    flag: u32,
) -> Vec<u8> {
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
            let cost = reduction.0;
            let mut node_bytes = node_to_bytes(
                &Node::new(&allocator, reduction.1)
            ).unwrap();

            node_bytes.splice(0..0, cost.to_be_bytes().to_vec());
            node_bytes
        },
        Err(_eval_err) => format!("{:?}", _eval_err).into(),
    }
}

#[wasm_bindgen]
pub fn vec_u8_to_u64(nums: &[u8]) -> u64 {
    u64::from_be_bytes(nums.try_into().expect("not enough bytes"))
}
