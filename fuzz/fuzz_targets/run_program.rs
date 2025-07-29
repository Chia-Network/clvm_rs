#![no_main]

use chia_fuzzing::make_tree_limits;
use libfuzzer_sys::fuzz_target;

use clvmr::allocator::Allocator;
use clvmr::chia_dialect::{ChiaDialect, MEMPOOL_MODE, NO_UNKNOWN_OPS};
use clvmr::cost::Cost;
use clvmr::reduction::Reduction;
use clvmr::run_program::run_program;

fuzz_target!(|data: &[u8]| {
    let mut unstructured = arbitrary::Unstructured::new(data);
    let mut allocator = Allocator::new();
    let (program, _) =
        make_tree_limits(&mut allocator, &mut unstructured, 10_000, true).expect("out of memory");
    let (args, _) =
        make_tree_limits(&mut allocator, &mut unstructured, 10_000, true).expect("out of memory");

    let allocator_checkpoint = allocator.checkpoint();

    for flags in [0, NO_UNKNOWN_OPS, MEMPOOL_MODE] {
        let dialect = ChiaDialect::new(flags);
        allocator.restore_checkpoint(&allocator_checkpoint);

        let Ok(Reduction(cost, _node)) = run_program(
            &mut allocator,
            &dialect,
            program,
            args,
            11_000_000_000 as Cost,
        ) else {
            continue;
        };
        assert!(cost < 11_000_000_000);
    }
});
