//! Per-eval overhead of `run_program_with_diagnostics` (the pre_eval/
//! post_eval capture client in `src/pre_eval_diagnostics.rs`) vs. plain
//! `run_program`, on the eval-heavy subset of `benches/run-program.rs`'s
//! own corpus (count-even, factorial, loop_add, matrix-multiply).
//!
//! Variants per program, same input/cost, same allocator-checkpoint
//! discipline as the baseline bench:
//! - `baseline`: plain `run_program` (control -- should match
//!   `run-program.rs`'s own numbers for the same test names).
//! - `diag-0`: `run_program_with_diagnostics(.., max_frames = 0)`.
//! - `diag-64`: `max_frames = 64`. The capture records every active frame
//!   and trims to `max_frames` only on failure, so `diag-0` and `diag-64`
//!   do the same per-eval work.
//! - `pre-only`: a `pre_eval` hook that returns `Ok(None)` -- the cost of
//!   one indirect call per eval, with no `PostEval` op scheduled.
//! - `noop-hooks`: a `pre_eval` hook returning a zero-sized, non-capturing
//!   `post_eval` -- the floor of the `pre_eval`/`post_eval` mechanism
//!   itself (indirect calls, `posteval_stack` push/pop, extra op), with
//!   no client work and no heap allocation.
//!
//! Requires the `pre-eval` cargo feature.

use clvmr::allocator::{Allocator, NodePtr};
use clvmr::chia_dialect::{ChiaDialect, ClvmFlags};
use clvmr::run_program;
use clvmr::run_program::{PostEval, PreEval, run_program_with_pre_eval};
use clvmr::run_program_with_diagnostics;
use criterion::{Criterion, SamplingMode, criterion_group, criterion_main};
use std::fs::read_to_string;
use std::time::Instant;

fn single_value<const N: i32>(a: &mut Allocator) -> NodePtr {
    let list = a.nil();
    let item = a.new_number(N.into()).expect("new_atom");
    a.new_pair(item, list).expect("new_pair")
}

fn generate_list<const N: i32>(a: &mut Allocator) -> NodePtr {
    let mut list = a.nil();
    for _i in 0..N {
        let item = a.new_number(42.into()).expect("new_atom");
        list = a.new_pair(item, list).expect("new_pair");
    }
    a.new_pair(list, a.nil()).expect("new_pair")
}

fn matrix<const W: i32, const H: i32>(a: &mut Allocator) -> NodePtr {
    let mut args = a.nil();
    for _l in 0..2 {
        let mut col = a.nil();
        for _k in 0..H {
            let mut row = a.nil();
            for _i in 0..W {
                let val = a.new_atom(b"ccba9401").expect("new_atom");
                row = a.new_pair(val, row).expect("new_pair");
            }
            col = a.new_pair(row, col).expect("new_pair");
        }
        args = a.new_pair(col, args).expect("new_pair");
    }
    args
}

type EnvFn = fn(&mut Allocator) -> NodePtr;

fn run_program_diagnostics_benchmark(c: &mut Criterion) {
    let mut a = Allocator::new();
    let dialect = ChiaDialect::new(ClvmFlags::ENABLE_GC);

    let test_case_checkpoint = a.checkpoint();

    let mut group = c.benchmark_group("run_program_diagnostics");
    group.sample_size(10);
    group.sampling_mode(SamplingMode::Flat);

    for (test, make_env) in &[
        ("count-even", generate_list::<15000> as EnvFn),
        ("factorial", single_value::<300>),
        ("loop_add", single_value::<3675000>),
        ("matrix-multiply", matrix::<50, 50>),
    ] {
        a.restore_checkpoint(&test_case_checkpoint);

        let prg = read_to_string(format!("benchmark/{test}.hex"))
            .expect("failed to load benchmark program");
        let prg = hex::decode(prg.trim()).expect("invalid hex in benchmark program");
        let max_cost = 11_000_000_000 - prg.len() as u64 * 12_000;
        let prg = clvmr::serde::node_from_bytes_backrefs(&mut a, &prg[..])
            .expect("failed to parse benchmark program");
        let env = make_env(&mut a);
        let iter_checkpoint = a.checkpoint();

        group.bench_function(format!("{test}/baseline"), |b| {
            b.iter(|| {
                a.restore_checkpoint(&iter_checkpoint);
                let start = Instant::now();
                run_program(&mut a, &dialect, prg, env, max_cost)
                    .expect("benchmark program failed");
                start.elapsed()
            })
        });

        group.bench_function(format!("{test}/diag-0"), |b| {
            b.iter(|| {
                a.restore_checkpoint(&iter_checkpoint);
                let start = Instant::now();
                run_program_with_diagnostics(&mut a, &dialect, prg, env, max_cost, 0)
                    .expect("benchmark program failed");
                start.elapsed()
            })
        });

        group.bench_function(format!("{test}/diag-64"), |b| {
            b.iter(|| {
                a.restore_checkpoint(&iter_checkpoint);
                let start = Instant::now();
                run_program_with_diagnostics(&mut a, &dialect, prg, env, max_cost, 64)
                    .expect("benchmark program failed");
                start.elapsed()
            })
        });

        group.bench_function(format!("{test}/pre-only"), |b| {
            b.iter(|| {
                a.restore_checkpoint(&iter_checkpoint);
                let start = Instant::now();
                let pre_eval: PreEval = Box::new(|_, _, _| Ok(None));
                run_program_with_pre_eval(&mut a, &dialect, prg, env, max_cost, Some(pre_eval))
                    .expect("benchmark program failed");
                start.elapsed()
            })
        });

        group.bench_function(format!("{test}/noop-hooks"), |b| {
            b.iter(|| {
                a.restore_checkpoint(&iter_checkpoint);
                let start = Instant::now();
                let pre_eval: PreEval = Box::new(|_, _, _| {
                    let post_eval: Box<PostEval> = Box::new(|_, _| {});
                    Ok(Some(post_eval))
                });
                run_program_with_pre_eval(&mut a, &dialect, prg, env, max_cost, Some(pre_eval))
                    .expect("benchmark program failed");
                start.elapsed()
            })
        });
    }

    group.finish();
}

criterion_group!(run_program_diagnostics, run_program_diagnostics_benchmark);
criterion_main!(run_program_diagnostics);
