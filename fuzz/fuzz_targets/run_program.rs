#![no_main]

use clvm_fuzzing::{make_clvm_program, make_tree_limits};
use libfuzzer_sys::{Corpus, fuzz_target};

use clvmr::allocator::Allocator;
use clvmr::chia_dialect::{ChiaDialect, ClvmFlags, MEMPOOL_MODE};
use clvmr::cost::Cost;
use clvmr::error::EvalErr;
use clvmr::reduction::Reduction;
use clvmr::run_program::run_program;

fuzz_target!(|data: &[u8]| -> Corpus {
    let mut unstructured = arbitrary::Unstructured::new(data);
    let mut allocator = Allocator::new();
    let (args, _) =
        make_tree_limits(&mut allocator, &mut unstructured, 100, true).expect("out of memory");
    let Ok(program) = make_clvm_program(&mut allocator, &mut unstructured, args, 100_000) else {
        return Corpus::Reject;
    };

    let allocator_checkpoint = allocator.checkpoint();

    for flags in [
        ClvmFlags::ENABLE_GC,
        ClvmFlags::empty(),
        ClvmFlags::NO_UNKNOWN_OPS,
        MEMPOOL_MODE,
    ] {
        let dialect = ChiaDialect::new(flags.union(ClvmFlags::DISABLE_OP));
        allocator.restore_checkpoint(&allocator_checkpoint);

        let result = run_program(
            &mut allocator,
            &dialect,
            program,
            args,
            11_000_000_000 as Cost,
        );

        match &result {
            Ok(Reduction(cost, _node)) => assert!(*cost < 11_000_000_000),
            Err(EvalErr::InternalError(..)) => {
                panic!("run_program returned InternalError: {:?}", result)
            }
            Err(_) => {}
        }
    }
    Corpus::Keep
});
