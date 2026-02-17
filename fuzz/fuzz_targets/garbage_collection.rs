#![no_main]

use clvm_fuzzing::{make_clvm_program, make_tree_limits, node_eq};
use libfuzzer_sys::{Corpus, fuzz_target};

use clvmr::allocator::Allocator;
use clvmr::chia_dialect::{ChiaDialect, ClvmFlags};
use clvmr::cost::Cost;
use clvmr::reduction::Reduction;
use clvmr::run_program::run_program;

const MAX_COST: Cost = 11_000_000_000;

fuzz_target!(|data: &[u8]| -> Corpus {
    let mut unstructured = arbitrary::Unstructured::new(data);
    let mut a = Allocator::new();
    let (args, _) = make_tree_limits(&mut a, &mut unstructured, 100, true).expect("out of memory");
    let Ok(program) = make_clvm_program(&mut a, &mut unstructured, args, 100_000) else {
        return Corpus::Reject;
    };

    // Ensure results are identical with and without ENABLE_GC
    let dialect = ChiaDialect::new(ClvmFlags::empty());
    let result_empty = run_program(&mut a, &dialect, program, args, MAX_COST);

    let dialect = ChiaDialect::new(ClvmFlags::ENABLE_GC);
    let result_gc = run_program(&mut a, &dialect, program, args, MAX_COST);

    match (&result_empty, &result_gc) {
        (Ok(Reduction(cost_empty, node_empty)), Ok(Reduction(cost_gc, node_gc))) => {
            assert_eq!(cost_empty, cost_gc, "cost differs empty vs ENABLE_GC");
            assert!(
                node_eq(&a, *node_empty, *node_gc),
                "result value differs empty vs ENABLE_GC"
            );
        }
        (Err(e_empty), Err(e_gc)) => {
            assert_eq!(
                e_empty.to_string(),
                e_gc.to_string(),
                "error differs empty vs ENABLE_GC"
            );
        }
        _ => panic!(
            "outcome mismatch: empty={} ENABLE_GC={}",
            result_empty.is_ok(),
            result_gc.is_ok()
        ),
    }
    Corpus::Keep
});
