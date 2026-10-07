//! Tests for `stack_trace::decode_failure_trace`.
//!
//! Two tiers:
//! - Default build: diagnostics tests asserted against explicitly-written
//!   expected traces (no oracle needed here -- these cases are simple
//!   enough to state as data), plus a success-path parity check.
//! - `--features pre-eval`: a capture-based oracle built on top of
//!   `run_program_with_pre_eval` (see the `oracle` module below), used to
//!   cross-validate the decoder against the hand-written wrinkle cases, the
//!   apply-heavy cases, and a seeded 1000-program random corpus. This
//!   machinery exists only here, under `cfg(test)` *and* the feature --
//!   production builds have none of it.

use crate::allocator::{Allocator, NodePtr};
use crate::chia_dialect::{ChiaDialect, ClvmFlags};
use crate::cost::Cost;
use crate::run_program::{EvalFailure, run_program, run_program_with_diagnostics};
use crate::stack_trace::{DecodedFrame, ProgramSource};
use crate::test_ops::{node_eq, parse_exp};

fn parse(a: &mut Allocator, s: &str) -> NodePtr {
    let (node, rest) = parse_exp(a, s);
    assert_eq!(rest.trim(), "", "trailing garbage parsing {s:?}");
    node
}

/// Run `prg`/`args` to failure through the production entry point.
///
/// Panics if the program actually succeeds -- every case in this file is
/// constructed to fail exactly once.
fn decode(prg: &str, args: &str, max_cost: Cost, flags: ClvmFlags) -> (Allocator, EvalFailure) {
    let mut a = Allocator::new();
    let program = parse(&mut a, prg);
    let env = parse(&mut a, args);
    let dialect = ChiaDialect::new(flags);
    match run_program_with_diagnostics(&mut a, &dialect, program, env, max_cost) {
        Ok(r) => panic!("expected {prg:?} / {args:?} to fail, got {r:?}"),
        Err(failure) => (a, failure),
    }
}

/// One frame's expected shape, for the explicit-expected-trace assertions
/// (default build, no oracle available).
#[derive(Debug)]
enum Expected {
    Source {
        program: &'static str,
        env: &'static str,
    },
    /// Never constructed by a current test case (none of the explicit
    /// hand-written traces hit the opaque-apply boundary -- those cases are
    /// covered by the oracle-validated `case_apply_of_cons_built_operator_opaque`
    /// instead), but kept so `assert_frames` can give a precise mismatch
    /// message if a future case needs it.
    #[allow(dead_code)]
    Opaque,
}

fn assert_frames(a: &mut Allocator, frames: &[DecodedFrame], expected: &[Expected]) {
    assert_eq!(
        frames.len(),
        expected.len(),
        "frame count: expected {expected:?}, got {frames:#?}"
    );
    for (i, (frame, exp)) in frames.iter().zip(expected).enumerate() {
        match (frame.program, exp) {
            (ProgramSource::Source(p), Expected::Source { program, env }) => {
                let expected_p = parse(a, program);
                assert!(
                    node_eq(a, p, expected_p),
                    "frame {i}: program mismatch, expected {program:?}"
                );
                let expected_e = parse(a, env);
                let got_env = frame
                    .environment
                    .unwrap_or_else(|| panic!("frame {i}: expected a resolved env"));
                assert!(
                    node_eq(a, got_env, expected_e),
                    "frame {i}: env mismatch, expected {env:?}"
                );
            }
            (ProgramSource::Opaque(_), Expected::Opaque) => {}
            (got, _) => panic!("frame {i}: expected {exp:?}-shaped frame, got {got:?}"),
        }
    }
}

// ---------------------------------------------------------------------
// 1. Diagnostics tests, asserted against explicitly-written expected
//    traces.
// ---------------------------------------------------------------------

#[test]
fn ported_nested_apply_frames_zero_residue() {
    // The fully-degenerate case: apply_kw's dispatch collapses the entire
    // call chain to zero stack residue (no sibling operands anywhere), so
    // the whole trace reconstructs from pure apply-of-quote source
    // inspection alone. This is the sharpest test of the quote-chasing path.
    let (mut a, failure) = decode(
        "(a (q . (/ 2 3)) (q . (10 . 0)))",
        "()",
        10_000,
        ClvmFlags::empty(),
    );
    assert_frames(
        &mut a,
        &failure.trace.frames,
        &[
            Expected::Source {
                program: "(a (q . (/ 2 3)) (q . (10 . 0)))",
                env: "()",
            },
            Expected::Source {
                program: "(/ 2 3)",
                env: "(10 . 0)",
            },
        ],
    );
}

#[test]
fn ported_removes_completed_siblings() {
    // `(> 3 3)` is a path-into-atom failure (env is nil, path 3 is a
    // leaf); the sibling `(q . 999)` operand completes before it and
    // leaves no trace.
    let (mut a, failure) = decode("(+ (> 3 3) (q . 999))", "()", 10_000, ClvmFlags::empty());
    assert_frames(
        &mut a,
        &failure.trace.frames,
        &[
            Expected::Source {
                program: "(+ (> 3 3) (q . 999))",
                env: "()",
            },
            Expected::Source {
                program: "(> 3 3)",
                env: "()",
            },
            // `>`'s own second operand is the bare atom `3`, which is
            // itself an env-path program; the path lookup against env `()`
            // is where the failure actually happens, one level deeper than
            // the `(> 3 3)` call itself.
            Expected::Source {
                program: "3",
                env: "()",
            },
        ],
    );
}

#[test]
fn ported_retains_newest_frames_when_truncated_shape() {
    // Nested apply-of-quote, two levels deep, into a path-into-atom
    // failure. This decoder never evicts or truncates anything, so there's
    // nothing bounded to check here beyond the full, untruncated shape.
    let (mut a, failure) = decode(
        "(a (q . (a (q . (> 3 3)) (q . ()))) (q . ()))",
        "()",
        10_000,
        ClvmFlags::empty(),
    );
    assert_frames(
        &mut a,
        &failure.trace.frames,
        &[
            Expected::Source {
                program: "(a (q . (a (q . (> 3 3)) (q . ()))) (q . ()))",
                env: "()",
            },
            Expected::Source {
                program: "(a (q . (> 3 3)) (q . ()))",
                env: "()",
            },
            Expected::Source {
                program: "(> 3 3)",
                env: "()",
            },
            Expected::Source {
                program: "3",
                env: "()",
            },
        ],
    );
}

#[test]
fn ported_pops_retained_frames_after_omission_shape() {
    let (mut a, failure) = decode(
        "(+ (> 3 3) (a (q . (a (q . 1) (q . ()))) (q . ())))",
        "()",
        10_000,
        ClvmFlags::empty(),
    );
    assert_frames(
        &mut a,
        &failure.trace.frames,
        &[
            Expected::Source {
                program: "(+ (> 3 3) (a (q . (a (q . 1) (q . ()))) (q . ())))",
                env: "()",
            },
            Expected::Source {
                program: "(> 3 3)",
                env: "()",
            },
            Expected::Source {
                program: "3",
                env: "()",
            },
        ],
    );
}

#[test]
fn ported_preserves_exact_error() {
    // `0x0fffffffff` isn't a valid apply target: apply_kw just evaluates
    // whatever atom it's given as an env path, so this is really an
    // oversized-number validation error with no frames below root.
    let (mut a, failure) = decode(
        "(a (q . 0x0fffffffff) (q . ()))",
        "()",
        10_000,
        ClvmFlags::empty(),
    );
    assert_frames(
        &mut a,
        &failure.trace.frames,
        &[
            Expected::Source {
                program: "(a (q . 0x0fffffffff) (q . ()))",
                env: "()",
            },
            // apply-of-quote resolves `P` to the oversized atom; evaluating
            // *that* as a program (an env-path lookup) is what actually
            // raises -- same one-level-deeper pattern as the path-into-atom
            // cases above.
            Expected::Source {
                program: "0x0fffffffff",
                env: "()",
            },
        ],
    );
}

// ---------------------------------------------------------------------
// 2. Success-path parity: the entry point must return exactly what plain
//    `run_program` does when the program succeeds.
// ---------------------------------------------------------------------

#[test]
fn success_parity_run_program_with_diagnostics() {
    let mut a = Allocator::new();
    let program = parse(&mut a, "(+ (q . 20) (q . 30))");
    let env = a.nil();
    let dialect = ChiaDialect::new(ClvmFlags::empty());

    let expected = run_program(&mut a, &dialect, program, env, 10_000).unwrap();
    let actual = run_program_with_diagnostics(&mut a, &dialect, program, env, 10_000).unwrap();
    assert_eq!(expected, actual);
}

// ---------------------------------------------------------------------
// 3. Pre-eval-feature-only: capture-based oracle, hand-written wrinkle
//    cases, apply-heavy cases, and the seeded random corpus.
// ---------------------------------------------------------------------

#[cfg(feature = "pre-eval")]
mod oracle_validated {
    use super::*;
    use crate::dialect::Dialect;
    use crate::reduction::Response;
    use crate::run_program::{PostEval, PreEval, run_program_with_pre_eval};
    use std::cell::RefCell;
    use std::rc::Rc;

    /// One frame as captured by `pre_eval`/`post_eval`: oldest to newest.
    type OracleFrames = Vec<(NodePtr, NodePtr)>;

    /// Run `program`/`env` through `run_program_with_pre_eval`, capturing
    /// the active `(program, env)` frames via a push/pop pre/post-eval
    /// pair: `pre_eval` fires on *every* `eval_pair` call and pushes;
    /// `post_eval` only fires when that same call completes successfully
    /// and pops. On `Err`, the eval loop abandons any pending `PostEval`
    /// ops still sitting on `op_stack` (it propagates the error via `?` and
    /// never drains the stack) -- so whatever is left in the vec when
    /// `run_program_with_pre_eval` returns `Err` is exactly the set of
    /// frames active at the moment of failure, same semantics the decoder
    /// is reconstructing from the raw stacks.
    fn run_with_oracle<D: Dialect>(
        allocator: &mut Allocator,
        dialect: &D,
        program: NodePtr,
        env: NodePtr,
        max_cost: Cost,
    ) -> (Response, OracleFrames) {
        let frames: Rc<RefCell<OracleFrames>> = Rc::new(RefCell::new(Vec::new()));
        let push_frames = frames.clone();
        let pre_eval: PreEval = Box::new(move |_allocator, program, env| {
            push_frames.borrow_mut().push((program, env));
            let pop_frames = push_frames.clone();
            let post_eval: Box<PostEval> = Box::new(move |_allocator, _result| {
                pop_frames.borrow_mut().pop();
            });
            Ok(Some(post_eval))
        });
        let result =
            run_program_with_pre_eval(allocator, dialect, program, env, max_cost, Some(pre_eval));
        let snapshot = frames.borrow().clone();
        (result, snapshot)
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Verdict {
        /// Every `Source` frame the decoder reported matched the oracle
        /// exactly, and both traces have the same length.
        ExactMatch,
        /// All `Source` frames matched, but the decoder's trace is shorter
        /// (it stopped at a documented wrinkle boundary: opaque apply, or
        /// ran out of stack+source evidence).
        PartialMatch,
        /// The decoder reported a `Source` frame that disagrees with the
        /// oracle's frame at the same position. This would be a real bug.
        Mismatch,
        /// The program didn't actually fail (generator slack).
        BothSucceeded,
    }

    struct CaseResult {
        description: String,
        verdict: Verdict,
        oracle_len: usize,
        decoded_len: usize,
        notes: Vec<String>,
    }

    /// Walk two trees that live in different `Allocator`s, comparing them
    /// structurally. `NodePtr`s are only ever handed to the `Allocator` they
    /// came from -- never compared or dereferenced against the other one --
    /// since under the `allocator-debug` feature a `NodePtr` carries an
    /// allocator fingerprint and using (or comparing) it against the wrong
    /// `Allocator` panics, regardless of whether the trees it points into
    /// are equal.
    fn cross_node_eq(a1: &Allocator, n1: NodePtr, a2: &Allocator, n2: NodePtr) -> bool {
        match (a1.sexp(n1), a2.sexp(n2)) {
            (crate::allocator::SExp::Pair(l1, r1), crate::allocator::SExp::Pair(l2, r2)) => {
                cross_node_eq(a1, l1, a2, l2) && cross_node_eq(a1, r1, a2, r2)
            }
            (crate::allocator::SExp::Atom, crate::allocator::SExp::Atom) => {
                a1.atom(n1).as_ref() == a2.atom(n2).as_ref()
            }
            _ => false,
        }
    }

    /// Run both the oracle and the decoder on *separately parsed* copies of
    /// the same program/env/max_cost, and compare their failure traces.
    ///
    /// Two independent allocators are used (one per side) rather than one
    /// shared allocator run twice: a single shared allocator run twice would
    /// have the second run start from an already-grown heap, so comparing
    /// its frames against the oracle's first run would be comparing nodes
    /// from two different points in the same heap's growth, not the same
    /// logical tree. Comparisons between the two allocators go through
    /// [`cross_node_eq`], which never uses a `NodePtr` against the
    /// `Allocator` that didn't create it.
    fn compare_case(
        description: &str,
        prg: &str,
        args: &str,
        max_cost: Cost,
        flags: ClvmFlags,
    ) -> CaseResult {
        let dialect = ChiaDialect::new(flags);

        let mut oracle_alloc = Allocator::new();
        let oracle_program = parse(&mut oracle_alloc, prg);
        let oracle_env = parse(&mut oracle_alloc, args);
        let (oracle_result, oracle_frames) = run_with_oracle(
            &mut oracle_alloc,
            &dialect,
            oracle_program,
            oracle_env,
            max_cost,
        );
        let oracle_err = match oracle_result {
            Ok(_) => {
                return CaseResult {
                    description: description.to_string(),
                    verdict: Verdict::BothSucceeded,
                    oracle_len: 0,
                    decoded_len: 0,
                    notes: vec![],
                };
            }
            Err(err) => err,
        };

        let mut allocator = Allocator::new();
        let program = parse(&mut allocator, prg);
        let env = parse(&mut allocator, args);
        let subject =
            run_program_with_diagnostics(&mut allocator, &dialect, program, env, max_cost);
        let failure = match subject {
            Ok(_) => panic!(
                "{description}: oracle failed with {oracle_err:?} but the subject (plain \
                 run_program_with_diagnostics) succeeded -- the decoder's harness must not \
                 change evaluation semantics"
            ),
            Err(failure) => failure,
        };

        // Compare by variant + display text rather than `==`: under
        // `allocator-debug`, `EvalErr`'s derived `PartialEq` would compare
        // the `NodePtr` some variants carry directly, and that assertion
        // rejects comparing `NodePtr`s from different allocators outright
        // (regardless of whether they happen to describe the same tree).
        // `EvalErr`'s `Display` only ever formats the `String` payload, not
        // the `NodePtr` one, so it's allocator-independent and exactly the
        // granularity this check needs: confirm both sides hit the same
        // kind of failure, not that their (incomparable) node identities
        // match.
        assert_eq!(
            std::mem::discriminant(&failure.error),
            std::mem::discriminant(&oracle_err),
            "{description}: run_program_with_diagnostics's error diverged from the pre-eval \
             oracle's (both are just run_program under the hood -- a real divergence here is a \
             bug, not a decoder limitation): {} vs {}",
            failure.error,
            oracle_err
        );
        assert_eq!(
            failure.error.to_string(),
            oracle_err.to_string(),
            "{description}: error message diverged between subject and oracle"
        );

        let mut notes = failure.trace.notes.clone();
        let mut verdict = Verdict::ExactMatch;
        let n = oracle_frames.len().min(failure.trace.frames.len());
        for (i, (&(oprogram, oenv), dframe)) in oracle_frames
            .iter()
            .zip(failure.trace.frames.iter())
            .enumerate()
            .take(n)
        {
            match dframe.program {
                ProgramSource::Source(p) => {
                    let program_ok = cross_node_eq(&allocator, p, &oracle_alloc, oprogram);
                    let env_ok = match dframe.environment {
                        Some(e) => cross_node_eq(&allocator, e, &oracle_alloc, oenv),
                        None => false,
                    };
                    if !program_ok || !env_ok {
                        verdict = Verdict::Mismatch;
                        notes.push(format!(
                            "frame {i}: decoder Source(program_ok={program_ok}, \
                             env_ok={env_ok}) disagreed with oracle"
                        ));
                    }
                }
                ProgramSource::Opaque(_) => {
                    notes.push(format!(
                        "frame {i}: decoder reported Opaque where oracle had a concrete frame \
                         (expected for apply-of-non-quote; otherwise a gap)"
                    ));
                    if verdict == Verdict::ExactMatch {
                        verdict = Verdict::PartialMatch;
                    }
                }
            }
        }
        if verdict == Verdict::ExactMatch && oracle_frames.len() != failure.trace.frames.len() {
            verdict = Verdict::PartialMatch;
            notes.push(format!(
                "frame count differs: oracle={}, decoder={}",
                oracle_frames.len(),
                failure.trace.frames.len()
            ));
        }

        CaseResult {
            description: description.to_string(),
            verdict,
            oracle_len: oracle_frames.len(),
            decoded_len: failure.trace.frames.len(),
            notes,
        }
    }

    fn assert_exact(result: &CaseResult) {
        assert_eq!(
            result.verdict,
            Verdict::ExactMatch,
            "{}: expected exact match, got {:?} (oracle {} frames, decoder {} frames): {:?}",
            result.description,
            result.verdict,
            result.oracle_len,
            result.decoded_len,
            result.notes
        );
    }

    // -----------------------------------------------------------------
    // Hand-written failure-mode cases
    // -----------------------------------------------------------------

    #[test]
    fn case_unknown_op_disallowed() {
        let r = compare_case(
            "unknown op, allow_unknown_ops off",
            "(+ (q . 1) (99 (q . 1)))",
            "()",
            10_000,
            ClvmFlags::NO_UNKNOWN_OPS,
        );
        assert_exact(&r);
    }

    #[test]
    fn case_cost_exceeded() {
        let r = compare_case(
            "cost exceeded (tight max_cost)",
            "(+ (q . 1) (q . 2))",
            "()",
            5, // too low to even cover QUOTE_COST*2 + OP_COST
            ClvmFlags::empty(),
        );
        assert_exact(&r);
    }

    #[test]
    fn case_bad_arg_count() {
        let r = compare_case(
            "bad arg count (f with 0 args)",
            "(+ (q . 1) (f))",
            "()",
            10_000,
            ClvmFlags::empty(),
        );
        assert_exact(&r);
    }

    #[test]
    fn case_double_pair_operator_form() {
        // `((X) ...)` syntax: operator position is itself a pair.
        let r = compare_case(
            "((X)...) operator form",
            "((+) (q . 1) (q . 2))",
            "()",
            10_000,
            ClvmFlags::empty(),
        );
        assert!(
            r.verdict != Verdict::Mismatch,
            "{}: {:?}",
            r.description,
            r.notes
        );
    }

    #[test]
    fn case_softfork_guarded_failure() {
        // softfork op: (softfork cost extension program env); use a
        // deliberately wrong declared cost to trigger SoftforkCostMismatch,
        // with a non-trivial guarded program so there's something nested to
        // decode.
        let r = compare_case(
            "softfork guard: cost mismatch",
            "(a (q 40 (q . 99999999) (q . 0) (q 16 (q . 1) (q . 2)) 1) 1)",
            "()",
            10_000,
            ClvmFlags::empty(),
        );
        assert!(
            r.verdict != Verdict::Mismatch,
            "{}: {:?}",
            r.description,
            r.notes
        );
    }

    #[test]
    fn case_path_into_atom_direct() {
        let r = compare_case(
            "path into atom, direct (env is nil, path 5)",
            "(+ (q . 1) 5)",
            "()",
            10_000,
            ClvmFlags::empty(),
        );
        assert_exact(&r);
    }

    #[test]
    fn case_apply_of_quote_recoverable() {
        let r = compare_case(
            "apply-of-quote: recoverable source chain, 2 levels",
            "(a (q . (a (q . (/ (q . 5) (q . 0))) (q . ()))) (q . ()))",
            "()",
            10_000,
            ClvmFlags::empty(),
        );
        assert_exact(&r);
    }

    #[test]
    fn case_apply_of_cons_built_operator_opaque() {
        // Currying pattern: the applied operator is built at runtime via
        // cons, not quoted -- genuinely opaque, no source position for the
        // dispatched program.
        let r = compare_case(
            "apply of cons-built operator (currying): opaque",
            "(a (c (q . 16) (c (q . 5) (q . ()))) (q . 7))",
            "()",
            10_000,
            ClvmFlags::empty(),
        );
        assert!(
            r.verdict != Verdict::Mismatch,
            "{}: {:?}",
            r.description,
            r.notes
        );
        assert!(
            r.notes.iter().any(|n| n.contains("opaque")),
            "{}: expected an opaque-case note, got {:?}",
            r.description,
            r.notes
        );
    }

    #[test]
    fn case_nested_applies_mixed() {
        let r = compare_case(
            "nested applies: quote chain into a real sibling failure",
            "(a (q . (+ (q . 1) (a (q . (f)) (q . ())))) (q . ()))",
            "()",
            10_000,
            ClvmFlags::empty(),
        );
        assert_exact(&r);
    }

    // -----------------------------------------------------------------
    // Random corpus
    // -----------------------------------------------------------------

    mod random_corpus {
        use super::*;
        use rand::Rng;
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        /// A tiny CLVM expression generator, biased toward producing a
        /// program that fails in exactly one, randomly-chosen way. Returns
        /// the whole program as a string in the same assembly syntax
        /// `test_ops::parse_exp` understands.
        struct Gen<'r> {
            rng: &'r mut StdRng,
            budget: u32,
            injected: bool,
        }

        impl<'r> Gen<'r> {
            fn new(rng: &'r mut StdRng, budget: u32) -> Self {
                Gen {
                    rng,
                    budget,
                    injected: false,
                }
            }

            /// A successful, side-effect-free leaf: a quoted small integer.
            fn quoted_leaf(&mut self) -> String {
                format!("(q . {})", self.rng.random_range(0..1000))
            }

            /// Build a node. `force_fail`, when true and nothing has been
            /// injected yet, guarantees this subtree contains the failure.
            fn node(&mut self, depth: u32, force_fail: bool) -> String {
                if depth == 0 || self.budget == 0 {
                    if force_fail && !self.injected {
                        return self.failing_leaf();
                    }
                    return self.quoted_leaf();
                }
                self.budget -= 1;

                let choice = if force_fail && !self.injected {
                    // Bias toward wrapping or directly injecting the failure.
                    self.rng.random_range(0..6)
                } else {
                    self.rng.random_range(0..4)
                };

                match choice {
                    0 => {
                        // (+ a b c) -- 2-4 args, at most one of which forces
                        // failure.
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
                        // apply-of-quote wrapper: (a (q . BODY) (q . ()))
                        let body = self.node(depth - 1, force_fail && !self.injected);
                        format!("(a (q . {body}) (q . ()))")
                    }
                    2 if force_fail && !self.injected => self.failing_leaf(),
                    2 => self.quoted_leaf(),
                    3 => {
                        // (i (q . 1) THEN ELSE) -- `if`, only THEN is live, so
                        // only put the forced failure there to keep
                        // single-failure determinism.
                        let then_branch = self.node(depth - 1, force_fail && !self.injected);
                        let else_branch = self.quoted_leaf();
                        format!("(i (q . 1) (q . {then_branch}) (q . {else_branch}))")
                    }
                    4 => {
                        // currying / apply-of-cons-built-operator (opaque
                        // case), applying `+` built at runtime to a quoted
                        // arg list.
                        self.injected = true;
                        "(a (c (q . 16) (c (q . 5) (c (q . 7) (q . ())))) (q . ()))".to_string()
                    }
                    _ => self.failing_leaf(),
                }
            }

            fn failing_leaf(&mut self) -> String {
                self.injected = true;
                match self.rng.random_range(0..4) {
                    0 => "(/ (q . 1) (q . 0))".to_string(), // division by zero
                    1 => "(99 (q . 1))".to_string(),        // unknown op
                    2 => "(f)".to_string(),                 // bad arg count
                    _ => "5".to_string(),                   // path into atom (env is nil)
                }
            }
        }

        #[derive(Default, Debug)]
        struct Stats {
            total: usize,
            exact: usize,
            partial: usize,
            mismatch: usize,
            both_succeeded: usize,
            note_counts: std::collections::BTreeMap<String, usize>,
        }

        fn classify_note(note: &str) -> &'static str {
            if note.contains("opaque") {
                "opaque-apply-of-non-quote"
            } else if note.contains("frame count differs") {
                "frame-count-mismatch"
            } else if note.contains("apply dispatch's env argument") {
                "unresolved-env-on-apply"
            } else if note.contains("stack pattern didn't parse") {
                "stack-pattern-unparsed"
            } else if note.contains("didn't match the frame's expected env") {
                "env-sanity-check-failed"
            } else if note.contains("expected an env_stack entry") {
                "env-stack-underflow"
            } else {
                "other"
            }
        }

        #[test]
        fn random_corpus_report() {
            let mut stats = Stats::default();
            let mut mismatches = Vec::new();

            for seed in 0..1000u64 {
                let mut rng = StdRng::seed_from_u64(seed);
                let prg_body = {
                    let mut generator = Gen::new(&mut rng, 6);
                    generator.node(5, true)
                };
                let prg = format!("(a (q . {prg_body}) (q . ()))");

                // Sanity-check the generated source parses before spending a
                // full compare_case on it.
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut probe = Allocator::new();
                    parse(&mut probe, &prg);
                }))
                .is_err()
                {
                    continue; // malformed generated source, skip
                }

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    compare_case(
                        &format!("random seed {seed}: {prg}"),
                        &prg,
                        "()",
                        10_000,
                        ClvmFlags::NO_UNKNOWN_OPS,
                    )
                }));

                let result = match result {
                    Ok(r) => r,
                    Err(payload) => {
                        let msg = payload
                            .downcast_ref::<String>()
                            .cloned()
                            .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                            .unwrap_or_else(|| "<panic>".to_string());
                        panic!("random seed {seed} ({prg}) panicked in compare_case: {msg}");
                    }
                };

                stats.total += 1;
                match result.verdict {
                    Verdict::ExactMatch => stats.exact += 1,
                    Verdict::PartialMatch => stats.partial += 1,
                    Verdict::Mismatch => {
                        stats.mismatch += 1;
                        mismatches.push(result.description.clone());
                    }
                    Verdict::BothSucceeded => stats.both_succeeded += 1,
                }
                for note in &result.notes {
                    *stats
                        .note_counts
                        .entry(classify_note(note).to_string())
                        .or_insert(0) += 1;
                }
            }

            eprintln!("\n=== stack-trace-decoder random corpus report ===");
            eprintln!("{stats:#?}");
            if !mismatches.is_empty() {
                eprintln!("--- mismatches (first 10) ---");
                for m in mismatches.iter().take(10) {
                    eprintln!("{m}");
                }
            }
            eprintln!("==================================================\n");

            assert_eq!(
                stats.mismatch, 0,
                "decoder disagreed with the oracle on a frame it claimed to resolve in {} / {} \
                 cases -- see stderr report",
                stats.mismatch, stats.total
            );
            assert!(
                stats.total > 900,
                "too many generated cases were skipped as malformed ({} / 1000 usable)",
                stats.total
            );
        }
    }
}
