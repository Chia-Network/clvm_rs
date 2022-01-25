use clvm_rs::wasm::api::run_clvm as wasm_run_clvm;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn run_clvm1(program: &[u8], args: &[u8]) -> Vec<u8> {
    wasm_run_clvm(program, args)
}
