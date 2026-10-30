//! Failure traces built on the existing pre-eval and post-eval callbacks.

use crate::allocator::{Allocator, NodePtr};
use crate::cost::Cost;
use crate::dialect::Dialect;
use crate::error::EvalErr;
use crate::reduction::Reduction;
use crate::run_program::{PostEval, PreEval, run_program_with_pre_eval};
use std::cell::RefCell;
use std::rc::Rc;

/// A raw CLVM evaluation frame active when evaluation failed.
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

/// Run a program and capture active evaluation frames on failure using the
/// existing `PreEval` and `PostEval` hooks.
///
/// Frames are ordered from oldest to newest. At most `max_frames` of the
/// newest active frames are returned; `truncated` counts omitted older frames.
/// This limit bounds the returned trace, not the stack recorded during execution.
pub fn run_program_with_diagnostics<D: Dialect>(
    allocator: &mut Allocator,
    dialect: &D,
    program: NodePtr,
    env: NodePtr,
    max_cost: Cost,
    max_frames: usize,
) -> std::result::Result<Reduction, EvalFailure> {
    let frames = Rc::new(RefCell::new(Vec::new()));
    let active_frames = Rc::clone(&frames);
    let pre_eval: PreEval = Box::new(move |_, program, environment| {
        active_frames.borrow_mut().push(EvalFrame {
            program,
            environment,
        });
        let active_frames = Rc::clone(&active_frames);
        let post_eval: Box<PostEval> = Box::new(move |_, _| {
            active_frames.borrow_mut().pop();
        });
        Ok(Some(post_eval))
    });
    let result =
        run_program_with_pre_eval(allocator, dialect, program, env, max_cost, Some(pre_eval));
    // On error, pending post-eval callbacks are dropped without being called.
    // The evaluator has now dropped all callbacks, leaving us sole ownership
    // of the frames whose evaluations were still active when it returned.
    let mut frames = Rc::try_unwrap(frames).unwrap().into_inner();
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

    #[test]
    fn diagnostics_success_matches_run_program() {
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
    fn diagnostics_preserves_exact_error() {
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
    fn diagnostics_nested_apply_frames() {
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
    fn diagnostics_omits_completed_siblings() {
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
    fn diagnostics_retains_newest_frames_when_truncated() {
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
    fn diagnostics_truncates_after_deep_completed_sibling() {
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
    fn diagnostics_reports_all_frames_after_deep_completed_sibling() {
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
    fn diagnostics_max_frames_zero_reports_no_frames() {
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

    #[test]
    fn diagnostics_cost_exceeded_in_nested_apply() {
        // Nested apply-of-quote with cost exhausted at a frame-setup
        // boundary: every reported frame is a currently-active frame, and
        // the outermost frame is reported first.
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
                // The outer apply-of-quote frame is pushed and never popped.
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
    fn diagnostics_cost_exceeded_in_small_atom_operators() {
        // Nested small-integer operators (`+` = 16, `-` = 17): the
        // reported frames start with the outermost program.
        let mut a = Allocator::new();
        let program = parse(&mut a, "(+ (q . 1) (a (q . (- (q . 5) (q . 2))) (q . ())))");
        let env = a.nil();
        let dialect = ChiaDialect::new(ClvmFlags::empty());
        let failure = match run_program_with_diagnostics(&mut a, &dialect, program, env, 5, 10) {
            Ok(r) => panic!("expected cost-exceeded failure, got {r:?}"),
            Err(failure) => failure,
        };
        // Cost 5 runs out before the outermost dispatch finishes.
        assert!(matches!(failure.error, EvalErr::CostExceeded));
        let outer = parse(&mut a, "(+ (q . 1) (a (q . (- (q . 5) (q . 2))) (q . ())))");
        if let Some(f0) = failure.frames.first() {
            assert!(node_eq(&a, f0.program, outer));
        }
    }
}
