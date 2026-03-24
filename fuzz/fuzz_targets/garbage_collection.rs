#![no_main]

use clvm_fuzzing::{make_clvm_program, make_tree_limits, node_eq_two};
use libfuzzer_sys::{Corpus, fuzz_target};

use clvmr::allocator::Allocator;
use clvmr::chia_dialect::{ChiaDialect, ClvmFlags};
use clvmr::cost::Cost;
use clvmr::reduction::{Reduction, Response};
use clvmr::run_program::run_program;

const MAX_COST: Cost = 11_000_000_000;

fuzz_target!(|data: &[u8]| -> Corpus {
    let mut results: Vec<(Response, Allocator)> = Vec::new();

    for flags in [ClvmFlags::empty(), ClvmFlags::ENABLE_GC] {
        let mut unstructured = arbitrary::Unstructured::new(data);
        let mut a = Allocator::new();
        let (args, _) =
            make_tree_limits(&mut a, &mut unstructured, 100, true).expect("out of memory");
        let Ok(program) = make_clvm_program(&mut a, &mut unstructured, args, 100_000) else {
            return Corpus::Reject;
        };
        let dialect = ChiaDialect::new(flags);
        let result = run_program(&mut a, &dialect, program, args, MAX_COST);
        results.push((result, a));
    }

    assert_eq!(
        results[0].1.atom_count(),
        results[1].1.atom_count(),
        "atom count differs empty vs ENABLE_GC"
    );
    assert_eq!(
        results[0].1.pair_count(),
        results[1].1.pair_count(),
        "pair count differs empty vs ENABLE_GC"
    );
    assert_eq!(
        results[0].1.heap_size(),
        results[1].1.heap_size(),
        "heap size differs empty vs ENABLE_GC"
    );

    match (&results[0].0, &results[1].0) {
        (Ok(Reduction(cost_empty, node_empty)), Ok(Reduction(cost_gc, node_gc))) => {
            assert_eq!(cost_empty, cost_gc, "cost differs empty vs ENABLE_GC");
            assert!(
                node_eq_two(&results[0].1, *node_empty, &results[1].1, *node_gc),
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
            results[0].0.is_ok(),
            results[1].0.is_ok()
        ),
    }
    Corpus::Keep
});
