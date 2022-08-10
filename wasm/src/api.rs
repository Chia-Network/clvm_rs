use wasm_bindgen::prelude::*;

use clvmr::allocator::Allocator;
use clvmr::chia_dialect::ChiaDialect;
use clvmr::chia_dialect::NO_NEG_DIV as _no_neg_div;
use clvmr::chia_dialect::NO_UNKNOWN_OPS as _no_unknown_ops;
use clvmr::cost::Cost;
use clvmr::node::Node;
use clvmr::reduction::Response;
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
    #[wasm_bindgen(getter)]
    pub fn no_neg_div(&self) -> u32 { _no_neg_div }

    #[wasm_bindgen(getter)]
    pub fn no_unknown_ops(&self) -> u32 { _no_unknown_ops }
}

#[wasm_bindgen]
pub fn serialized_length(program: &[u8]) -> u64 {
    serialized_length_from_bytes(program).unwrap()
}

fn _run_program(
    allocator: &mut Allocator,
    program: &[u8],
    args: &[u8],
    max_cost: Cost,
    flag: u32,
) -> Response {
    let program = node_from_bytes(allocator, program).unwrap();
    let args = node_from_bytes(allocator, args).unwrap();
    let dialect = ChiaDialect::new(flag);

    run_program(
        allocator,
        &dialect,
        program,
        args,
        max_cost,
        None,
    )
}

#[wasm_bindgen]
pub fn run_clvm(program: &[u8], args: &[u8]) -> Vec<u8> {
    let max_cost: Cost = 1_000_000_000_000_000;

    let mut allocator = Allocator::new();
    let r = _run_program(
        &mut allocator,
        program,
        args,
        max_cost,
        0,
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
    let r = _run_program(
        &mut allocator,
        program,
        args,
        max_cost,
        flag,
    );
    match r {
        Ok(reduction) => {
            let cost = reduction.0;
            let node_bytes = node_to_bytes(
                &Node::new(&allocator, reduction.1)
            ).unwrap();

            [
                u64_to_vec_u8(cost),
                node_bytes,
            ].concat()
        },
        Err(_eval_err) => format!("{:?}", _eval_err).into(),
    }
}

#[wasm_bindgen]
pub fn u64_to_vec_u8(num: u64) -> Vec<u8> {
    let u8_vec = vec![
        ((num & 0xff000000_00000000) >> 56) as u8,
        ((num & 0x00ff0000_00000000) >> 48) as u8,
        ((num & 0x0000ff00_00000000) >> 40) as u8,
        ((num & 0x000000ff_00000000) >> 32) as u8,
        ((num & 0xff000000) >> 24) as u8,
        ((num & 0x00ff0000) >> 16) as u8,
        ((num & 0x0000ff00) >> 8) as u8,
        ((num & 0x000000ff) >> 0) as u8,
    ];

    u8_vec
}

#[wasm_bindgen]
pub fn vec_u8_to_u64(nums: &[u8]) -> u64 {
    let u64_value =
        ((nums[0] as u64) << 56)
            + ((nums[1] as u64) << 48)
            + ((nums[2] as u64) << 40)
            + ((nums[3] as u64) << 32)
            + ((nums[4] as u64) << 24)
            + ((nums[5] as u64) << 16)
            + ((nums[6] as u64) << 8)
            + (nums[7] as u64)
        ;

    u64_value
}
