//! Failure-trace capture built as an `EvalHooks` client of the evaluator.
//!
//! This is one of three competing designs for recovering the active
//! `(program, env)` call frames at the point an evaluation fails:
//!
//! - PR #845: in-core bookkeeping — the evaluator itself pushes/pops a
//!   `DiagnosticState` via a dedicated `Operation::PopDiagnosticFrame`
//!   variant, widening the hot `Operation` enum for every build.
//! - PR #847: a read-only decoder that reconstructs the trace from
//!   `op_stack`/`val_stack`/`env_stack` residue after `run_program` returns
//!   `Err`. Zero steady-state cost, but the reconstruction is an inference
//!   problem with its own correctness subtleties (fast-path/zero-residue
//!   and operator-identity wrinkles).
//! - This module: push the frame in `pre_eval`, pop it in `post_eval`.
//!   `post_eval` only fires when its matching `eval_pair` call completes
//!   *successfully* — on `Err` the evaluator's `?`-propagation abandons
//!   whatever `PostEval` ops are still pending on `op_stack` without
//!   draining them, so whatever this module's bookkeeping has not yet
//!   popped when `run_program_with_hooks` returns `Err` is exactly the
//!   set of frames active at the moment of failure. No inference: every
//!   frame recorded here was handed directly to a hook by the evaluator.
//!
//! The bookkeeping is a plain stack of every active frame. #845's bounded
//! eviction/restore algorithm always retains exactly the newest
//! `min(depth, max_frames)` active frames and parks each evicted frame on
//! the evaluator's op stack until its evicting frame pops, so its total
//! storage is also one entry per active frame. Keeping the whole stack
//! here and trimming to the newest `max_frames` on failure yields the same
//! `frames`/`truncated` as #845.
//!
//! The hooks are a statically dispatched `EvalHooks` impl that owns the
//! frame stack, so per `eval_pair` the cost is an inlined `Vec` push/pop
//! plus one extra op on the evaluator's op stack. The build must enable
//! the `pre-eval` cargo feature.

use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::dialect::Dialect;
use crate::error::{EvalErr, Result};
use crate::reduction::Reduction;
use crate::run_program::{EvalHooks, run_program_with_hooks};

/// A raw CLVM evaluation frame active when evaluation failed.
///
/// Field-for-field identical to PR #845's `EvalFrame`: both designs hand
/// out bare, already-computed `NodePtr`s (not an opaque/source-position
/// wrapper like #847's decoder), so downstreams can treat the two as
/// drop-in replacements for one another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EvalFrame {
    /// The program being evaluated.
    pub program: NodePtr,
    /// The environment the program is evaluated in.
    pub environment: NodePtr,
}

/// An evaluation error together with the active evaluation frames.
#[derive(Debug, PartialEq)]
pub struct EvalFailure {
    /// The original evaluator error.
    pub error: EvalErr,
    /// Retained active frames, ordered from oldest to newest.
    pub frames: Vec<EvalFrame>,
    /// The number of older active frames omitted from `frames`.
    pub truncated: usize,
}

/// The result of an evaluator run with failure diagnostics enabled.
pub type DiagnosticResponse = std::result::Result<Reduction, EvalFailure>;

/// Records every active frame, oldest first.
struct FrameCapture {
    frames: Vec<EvalFrame>,
}

impl EvalHooks for FrameCapture {
    #[inline(always)]
    fn pre_eval(
        &mut self,
        _allocator: &mut Allocator,
        program: NodePtr,
        environment: NodePtr,
    ) -> Result<bool> {
        self.frames.push(EvalFrame {
            program,
            environment,
        });
        Ok(true)
    }

    #[inline(always)]
    fn post_eval(&mut self, _allocator: &mut Allocator, _result: Option<NodePtr>) {
        self.frames.pop();
    }
}

/// Run a program and capture the active evaluation frames on failure, via
/// an [`EvalHooks`] client of [`run_program_with_hooks`].
///
/// Frames are ordered from oldest to newest. At most `max_frames` of the
/// most recent active frames are retained, and `truncated` reports the
/// number of older active frames omitted from the result. Matches PR
/// #845's `run_program_with_diagnostics` signature and `EvalFailure`/
/// `EvalFrame` shapes exactly.
pub fn run_program_with_diagnostics<D: Dialect>(
    allocator: &mut Allocator,
    dialect: &D,
    program: NodePtr,
    env: NodePtr,
    max_cost: Cost,
    max_frames: usize,
) -> DiagnosticResponse {
    let mut capture = FrameCapture { frames: Vec::new() };
    let result = run_program_with_hooks(allocator, dialect, program, env, max_cost, &mut capture);
    let mut frames = capture.frames;
    match result {
        Ok(reduction) => Ok(reduction),
        Err(error) => {
            let truncated = frames.len().saturating_sub(max_frames);
            frames.drain(..truncated);
            Err(EvalFailure {
                error,
                frames,
                truncated,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chia_dialect::{ChiaDialect, ClvmFlags};
    use crate::reduction::Reduction;
    use crate::run_program::run_program;
    use crate::test_ops::{node_eq, parse_exp};

    fn parse(a: &mut Allocator, s: &str) -> NodePtr {
        let (node, rest) = parse_exp(a, s);
        assert_eq!(rest.trim(), "", "trailing garbage parsing {s:?}");
        node
    }

    // ---------------------------------------------------------------
    // Ported verbatim from PR #845 (fetched at `9a86ca42`,
    // `src/run_program.rs` `test_diagnostics_*` family -- the 7 tests
    // exercising `run_program_with_diagnostics`/`EvalFailure`/
    // `EvalFrame`, excluding `test_run_program_uses_non_diagnostic_context`
    // which only checks that plain `run_program` doesn't carry diagnostic
    // bookkeeping and has no analog in a client-side design). Same
    // programs, same `max_cost`/`max_frames`, same expected
    // frames/truncation -- run here against this module's
    // `run_program_with_diagnostics` instead of #845's in-core one.
    // ---------------------------------------------------------------

    #[test]
    fn ported_diagnostics_success_matches_run_program() {
        let mut allocator = Allocator::new();
        let program = parse(&mut allocator, "(+ (q . 20) (q . 30))");
        let env = allocator.nil();
        let dialect = ChiaDialect::new(ClvmFlags::empty());

        let Reduction(expected_cost, expected_value) =
            run_program(&mut allocator, &dialect, program, env, 10000).unwrap();
        let Reduction(actual_cost, actual_value) =
            run_program_with_diagnostics(&mut allocator, &dialect, program, env, 10000, 10)
                .unwrap();

        assert_eq!(actual_cost, expected_cost);
        assert!(node_eq(&allocator, actual_value, expected_value));
    }

    #[test]
    fn ported_diagnostics_preserves_exact_error() {
        let mut allocator = Allocator::new();
        let program = parse(&mut allocator, "(a (q . 0x0fffffffff) (q . ()))");
        let env = allocator.nil();
        let dialect = ChiaDialect::new(ClvmFlags::empty());

        let expected = run_program(&mut allocator, &dialect, program, env, 10000).unwrap_err();
        let failure =
            run_program_with_diagnostics(&mut allocator, &dialect, program, env, 10000, 10)
                .unwrap_err();

        assert_eq!(failure.error, expected);
    }

    #[test]
    fn ported_diagnostics_nested_apply_frames() {
        let mut allocator = Allocator::new();
        let program = parse(&mut allocator, "(a (q . (/ 2 3)) (q . (10 . 0)))");
        let env = allocator.nil();
        let dialect = ChiaDialect::new(ClvmFlags::empty());

        let failure =
            run_program_with_diagnostics(&mut allocator, &dialect, program, env, 10000, 10)
                .unwrap_err();
        let inner_program = parse(&mut allocator, "(/ 2 3)");
        let inner_env = parse(&mut allocator, "(10 . 0)");

        assert_eq!(failure.error.to_string(), "Division by zero");
        assert_eq!(failure.truncated, 0);
        assert_eq!(failure.frames.len(), 2);
        assert_eq!(
            failure.frames[0],
            EvalFrame {
                program,
                environment: env
            }
        );
        assert!(node_eq(
            &allocator,
            failure.frames[1].program,
            inner_program
        ));
        assert!(node_eq(
            &allocator,
            failure.frames[1].environment,
            inner_env
        ));
    }

    #[test]
    fn ported_diagnostics_removes_completed_siblings() {
        let mut allocator = Allocator::new();
        let program = parse(&mut allocator, "(+ (> 3 3) (q . 999))");
        let completed_sibling = parse(&mut allocator, "(q . 999)");
        let env = allocator.nil();
        let dialect = ChiaDialect::new(ClvmFlags::empty());

        let failure =
            run_program_with_diagnostics(&mut allocator, &dialect, program, env, 10000, 10)
                .unwrap_err();

        assert_eq!(failure.error, EvalErr::PathIntoAtom);
        assert!(failure.frames.iter().all(|frame| !node_eq(
            &allocator,
            frame.program,
            completed_sibling
        )));
    }

    #[test]
    fn ported_diagnostics_retains_newest_frames_when_truncated() {
        let mut allocator = Allocator::new();
        let program = parse(
            &mut allocator,
            "(a (q . (a (q . (> 3 3)) (q . ()))) (q . ()))",
        );
        let env = allocator.nil();
        let dialect = ChiaDialect::new(ClvmFlags::empty());

        let failure =
            run_program_with_diagnostics(&mut allocator, &dialect, program, env, 10000, 2)
                .unwrap_err();
        let comparison = parse(&mut allocator, "(> 3 3)");
        let failing_path = parse(&mut allocator, "3");

        assert_eq!(failure.error, EvalErr::PathIntoAtom);
        assert_eq!(failure.truncated, 2);
        assert_eq!(failure.frames.len(), 2);
        assert!(node_eq(&allocator, failure.frames[0].program, comparison));
        assert!(node_eq(&allocator, failure.frames[1].program, failing_path));
    }

    #[test]
    fn ported_diagnostics_pops_retained_frames_after_omission() {
        let mut allocator = Allocator::new();
        let program = parse(
            &mut allocator,
            "(+ (> 3 3) (a (q . (a (q . 1) (q . ()))) (q . ())))",
        );
        let env = allocator.nil();
        let dialect = ChiaDialect::new(ClvmFlags::empty());

        let failure =
            run_program_with_diagnostics(&mut allocator, &dialect, program, env, 10000, 2)
                .unwrap_err();
        let comparison = parse(&mut allocator, "(> 3 3)");
        let failing_path = parse(&mut allocator, "3");

        assert_eq!(failure.error, EvalErr::PathIntoAtom);
        assert_eq!(failure.truncated, 1);
        assert_eq!(failure.frames.len(), 2);
        assert!(node_eq(&allocator, failure.frames[0].program, comparison));
        assert!(node_eq(&allocator, failure.frames[1].program, failing_path));
    }

    #[test]
    fn ported_diagnostics_restores_ancestors_after_deep_completed_sibling() {
        let mut allocator = Allocator::new();
        let program = parse(
            &mut allocator,
            "(+ (> 3 3) (a (q . (a (q . 1) (q . ()))) (q . ())))",
        );
        let env = allocator.nil();
        let dialect = ChiaDialect::new(ClvmFlags::empty());

        let failure =
            run_program_with_diagnostics(&mut allocator, &dialect, program, env, 10000, 3)
                .unwrap_err();

        assert_eq!(failure.error, EvalErr::PathIntoAtom);
        assert_eq!(failure.truncated, 0);
        assert_eq!(failure.frames.len(), 3);
    }

    #[test]
    fn ported_diagnostics_max_frames_zero_is_no_bookkeeping() {
        let mut allocator = Allocator::new();
        let program = parse(&mut allocator, "(+ (> 3 3) (q . 999))");
        let env = allocator.nil();
        let dialect = ChiaDialect::new(ClvmFlags::empty());

        let failure =
            run_program_with_diagnostics(&mut allocator, &dialect, program, env, 10000, 0)
                .unwrap_err();
        assert_eq!(failure.frames.len(), 0);
        assert_eq!(failure.truncated, 3);
    }

    // ---------------------------------------------------------------
    // Regression cases shaped like PR #847's two decoder bugs. This
    // capture does no stack-residue inference at all -- every frame
    // comes straight from a pre_eval/post_eval hook call -- so neither
    // bug class has an analog here. These cases exist to prove that,
    // not because this code path is suspected of having them.
    // ---------------------------------------------------------------

    #[test]
    fn regression_cost_exceeded_mid_frame_setup_apply_of_quote() {
        // Shape of #847's fast-path/zero-residue bug: nested
        // (a (q . (a ...)) (q . ...)) with cost exhausted right around a
        // frame-setup boundary. #847's decoder can hop past a still-live
        // frame here because it trusts "both apply operands quoted" as a
        // proxy for "already fully dispatched" without checking op_stack.
        // This capture never infers from op_stack shape -- the frame
        // trace is exactly whatever pre_eval calls fired -- so cost
        // exhaustion can only ever truncate the *reported* trace via the
        // `truncated` counter, never fabricate a wrong frame.
        let mut a = Allocator::new();
        let program = parse(
            &mut a,
            "(a (q . (a (q . (+ (q . 1) (q . 1))) (q . ()))) (q . ()))",
        );
        let env = a.nil();
        let dialect = ChiaDialect::new(ClvmFlags::empty());
        // Tight enough to exhaust mid-dispatch, inside the outer apply's
        // frame-setup, before the inner apply even gets evaluated.
        let max_cost = 150;
        match run_program_with_diagnostics(&mut a, &dialect, program, env, max_cost, 10) {
            Ok(r) => panic!("expected cost-exceeded failure, got {r:?}"),
            Err(failure) => {
                assert!(matches!(failure.error, EvalErr::CostExceeded));
                // Every reported frame must be a real, currently-active
                // (program, env) pair pushed by pre_eval -- not a
                // fabricated hop. The outer apply-of-quote frame must be
                // present (it was pushed and never popped).
                assert!(
                    !failure.frames.is_empty(),
                    "expected at least the outer frame to be active when cost ran out"
                );
                let outer = parse(
                    &mut a,
                    "(a (q . (a (q . (+ (q . 1) (q . 1))) (q . ()))) (q . ()))",
                );
                assert!(node_eq(&a, failure.frames[0].program, outer));
            }
        }
    }

    #[test]
    fn regression_smallatom_operator_identity_shaped() {
        // Shape of #847's identity-collision bug: the decoder's
        // operator-NodePtr identity check on val_stack is unsound for
        // SmallAtom-encoded operators (small integer operator codes can
        // collide in identity with unrelated small-atom values elsewhere
        // on the stack). This capture performs no NodePtr identity
        // comparison of any kind -- frames are keyed by nothing but
        // "a pre_eval call happened and hasn't been matched by post_eval
        // yet" -- so there's no identity check to collide. Use two
        // small-integer operators (`+` = 16, `-` = 17) nested so a
        // naive identity-based decoder has operator atoms to confuse.
        let mut a = Allocator::new();
        let program = parse(&mut a, "(+ (q . 1) (a (q . (- (q . 5) (q . 2))) (q . ())))");
        let env = a.nil();
        let dialect = ChiaDialect::new(ClvmFlags::empty());
        let failure = match run_program_with_diagnostics(&mut a, &dialect, program, env, 5, 10) {
            Ok(r) => panic!("expected cost-exceeded failure, got {r:?}"),
            Err(failure) => failure,
        };
        // Cost 5 is too tight to even finish the outermost dispatch; the
        // point of this case is just that whatever frames ARE reported
        // are real and in the right order, not fabricated via identity
        // confusion between the `+` (16) and `-` (17) operator atoms.
        assert!(matches!(failure.error, EvalErr::CostExceeded));
        let outer = parse(&mut a, "(+ (q . 1) (a (q . (- (q . 5) (q . 2))) (q . ())))");
        if let Some(f0) = failure.frames.first() {
            assert!(node_eq(&a, f0.program, outer));
        }
    }

    // ---------------------------------------------------------------
    // Corpus export for cross-worktree comparison against PR #847's
    // decoder (see ~/projects/clvm_rs/stack-trace-decoder). This test is
    // `#[ignore]`d (it writes a file, not an assertion) -- run it
    // explicitly:
    //
    //   cargo test --features pre-eval -- --ignored export_corpus
    //
    // It uses the *same* seeded generator as the decoder's own
    // `random_corpus` module in `stack_trace_tests.rs` (ported verbatim,
    // same seeds 0..1000, same `Gen` shape, same injected failure
    // modes), so seed N produces byte-identical source on both sides.
    // For each case it runs this module's `run_program_with_diagnostics`
    // (max_frames large enough to never evict, so every active frame is
    // reported) and writes one TSV line: seed, source, outcome
    // (`ok`/`err:<Display>`), and each frame's program/env serialized via
    // `node_to_bytes` (hex) so a separate process (a different Allocator,
    // a different worktree) can reconstruct and compare them without
    // ever comparing a `NodePtr` across allocators.
    #[test]
    #[ignore = "writes /tmp/pre-eval-capture-corpus.tsv; run explicitly for the cross-worktree decoder comparison"]
    fn export_corpus() {
        use crate::serde::node_to_bytes;
        use corpus_gen::Gen;
        use rand::SeedableRng;
        use rand::rngs::StdRng;
        use std::fmt::Write as _;
        use std::io::Write as _;

        let mut out = String::new();
        for seed in 0..1000u64 {
            let mut rng = StdRng::seed_from_u64(seed);
            let prg_body = {
                let mut generator = Gen::new(&mut rng, 6);
                generator.node(5, true)
            };
            let prg = format!("(a (q . {prg_body}) (q . ()))");

            let mut a = Allocator::new();
            let probe =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| parse(&mut a, &prg)));
            let Ok(program) = probe else {
                continue; // malformed generated source, skip (generator slack)
            };
            let env = a.nil();
            let dialect = ChiaDialect::new(ClvmFlags::NO_UNKNOWN_OPS);

            write!(out, "{seed}\t{prg}\t").unwrap();
            match run_program_with_diagnostics(&mut a, &dialect, program, env, 10_000, 10_000) {
                Ok(_) => {
                    writeln!(out, "ok\t").unwrap();
                }
                Err(failure) => {
                    write!(out, "err:{}\t", failure.error).unwrap();
                    let mut parts = Vec::new();
                    for frame in &failure.frames {
                        let p = hex::encode(node_to_bytes(&a, frame.program).unwrap());
                        let e = hex::encode(node_to_bytes(&a, frame.environment).unwrap());
                        parts.push(format!("{p},{e}"));
                    }
                    writeln!(out, "{}", parts.join(";")).unwrap();
                }
            }
        }

        let path = "/tmp/pre-eval-capture-corpus.tsv";
        std::fs::File::create(path)
            .unwrap()
            .write_all(out.as_bytes())
            .unwrap();
        eprintln!("wrote corpus to {path}");
    }
}

/// Ported from `stack-trace-decoder`'s
/// `src/stack_trace_tests.rs::oracle_validated::random_corpus::Gen`
/// (seeded 1000-program random generator biased toward exactly one
/// injected failure). Kept in a submodule so `export_corpus` above can
/// use it without pulling it into the main `tests` namespace.
#[cfg(test)]
mod corpus_gen {
    use rand::Rng;
    use rand::rngs::StdRng;

    pub struct Gen<'r> {
        rng: &'r mut StdRng,
        budget: u32,
        injected: bool,
    }

    impl<'r> Gen<'r> {
        pub fn new(rng: &'r mut StdRng, budget: u32) -> Self {
            Gen {
                rng,
                budget,
                injected: false,
            }
        }

        fn quoted_leaf(&mut self) -> String {
            format!("(q . {})", self.rng.random_range(0..1000))
        }

        pub fn node(&mut self, depth: u32, force_fail: bool) -> String {
            if depth == 0 || self.budget == 0 {
                if force_fail && !self.injected {
                    return self.failing_leaf();
                }
                return self.quoted_leaf();
            }
            self.budget -= 1;

            let choice = if force_fail && !self.injected {
                self.rng.random_range(0..6)
            } else {
                self.rng.random_range(0..4)
            };

            match choice {
                0 => {
                    let n = self.rng.random_range(2..=4);
                    let fail_slot = if force_fail && !self.injected {
                        Some(self.rng.random_range(0..n))
                    } else {
                        None
                    };
                    let mut parts = Vec::new();
                    for i in 0..n {
                        parts.push(self.node(depth - 1, Some(i) == fail_slot));
                    }
                    format!("(+ {})", parts.join(" "))
                }
                1 => {
                    let body = self.node(depth - 1, force_fail && !self.injected);
                    format!("(a (q . {body}) (q . ()))")
                }
                2 if force_fail && !self.injected => self.failing_leaf(),
                2 => self.quoted_leaf(),
                3 => {
                    let then_branch = self.node(depth - 1, force_fail && !self.injected);
                    let else_branch = self.quoted_leaf();
                    format!("(i (q . 1) (q . {then_branch}) (q . {else_branch}))")
                }
                4 => {
                    self.injected = true;
                    "(a (c (q . 16) (c (q . 5) (c (q . 7) (q . ())))) (q . ()))".to_string()
                }
                _ => self.failing_leaf(),
            }
        }

        fn failing_leaf(&mut self) -> String {
            self.injected = true;
            match self.rng.random_range(0..4) {
                0 => "(/ (q . 1) (q . 0))".to_string(),
                1 => "(99 (q . 1))".to_string(),
                2 => "(f)".to_string(),
                _ => "5".to_string(),
            }
        }
    }
}
