#![no_main]
use libfuzzer_sys::fuzz_target;

use clvm_rs::chia_dialect::ChiaDialect;
use clvm_rs::allocator::Allocator;
use clvm_rs::reduction::Reduction;
use clvm_rs::serialize::node_from_bytes;
use clvm_rs::cost::Cost;
use clvm_rs::run_program::run_program;

fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let program = match node_from_bytes(&mut allocator, data) {
        Err(_) => { return; },
        Ok(r) => r,
    };
    let args = allocator.null();
    let dialect = ChiaDialect::new(false);

    let Reduction(_cost, _node) = match run_program(&mut allocator, &dialect, program, args, 12000000000 as Cost, None) {
        Err(_) => { return; },
        Ok(r) => r,
    };
});
