#![no_main]
use libfuzzer_sys::fuzz_target;

use clvmr::allocator::Allocator;
use clvmr::chia_dialect::{
    ChiaDialect, ENABLE_BLS_OPS_OUTSIDE_GUARD, MEMPOOL_MODE, NO_UNKNOWN_OPS,
};
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
    let args = allocator.nil();

    let allocator_checkpoint = allocator.checkpoint();

    for flags in [
        0,
        ENABLE_BLS_OPS_OUTSIDE_GUARD,
        ENABLE_BLS_OPS_OUTSIDE_GUARD | NO_UNKNOWN_OPS,
        MEMPOOL_MODE,
    ] {
        let dialect = ChiaDialect::new(flags);
        allocator.restore_checkpoint(&allocator_checkpoint);

        let Reduction(_cost, _node) =
            match run_program(&mut allocator, &dialect, program, args, 11000000000 as Cost) {
                Err(_) => {
                    continue;
                }
                Ok(r) => r,
            };
    }
});
