#![no_main]
use libfuzzer_sys::fuzz_target;

use clvmr::allocator::Allocator;
use clvmr::chia_dialect::ChiaDialect;
use clvmr::cost::Cost;
use clvmr::reduction::Reduction;
use clvmr::run_program::run_program;
use clvmr::serde::node_from_bytes;

fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let program = match node_from_bytes(&mut allocator, data) {
        Err(_) => {
            return;
        }
        Ok(r) => r,
    };
    let args = allocator.null();
    let dialect = ChiaDialect::new(0);

    let Reduction(_cost, _node) = match run_program(
        &mut allocator,
        &dialect,
        program,
        args,
        12000000000 as Cost,
    ) {
        Err(_) => {
            return;
        }
        Ok(r) => r,
    };
});
