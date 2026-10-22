use crate::allocator::{Allocator, NodePtr};
use crate::error::Result;
#[cfg(feature = "pre-eval")]
use crate::run_program::{PostEval, PreEval};

/// Callbacks the evaluator makes around every `eval_pair`, dispatched
/// statically through the evaluator's type parameter.
pub trait EvalHooks {
    /// Called before `program` is evaluated in `env`. Returning `true`
    /// schedules a matching `post_eval` call for when this evaluation
    /// completes successfully; an `Err` aborts the run.
    fn pre_eval(
        &mut self,
        allocator: &mut Allocator,
        program: NodePtr,
        env: NodePtr,
    ) -> Result<bool>;

    /// Called with the top of the value stack when an evaluation whose
    /// `pre_eval` returned `true` completes. Calls nest like the
    /// evaluations they belong to.
    fn post_eval(&mut self, allocator: &mut Allocator, result: Option<NodePtr>);
}

impl<H: EvalHooks + ?Sized> EvalHooks for &mut H {
    #[inline(always)]
    fn pre_eval(
        &mut self,
        allocator: &mut Allocator,
        program: NodePtr,
        env: NodePtr,
    ) -> Result<bool> {
        (**self).pre_eval(allocator, program, env)
    }

    #[inline(always)]
    fn post_eval(&mut self, allocator: &mut Allocator, result: Option<NodePtr>) {
        (**self).post_eval(allocator, result)
    }
}

/// The hooks of plain `run_program`: no callbacks scheduled.
pub struct NoHooks;

impl EvalHooks for NoHooks {
    #[inline(always)]
    fn pre_eval(&mut self, _: &mut Allocator, _: NodePtr, _: NodePtr) -> Result<bool> {
        Ok(false)
    }

    #[inline(always)]
    fn post_eval(&mut self, _: &mut Allocator, _: Option<NodePtr>) {}
}

/// Adapts the boxed-closure `PreEval`/`PostEval` API to `EvalHooks`.
#[cfg(feature = "pre-eval")]
pub struct BoxedHooks {
    pre_eval: PreEval,
    posteval_stack: Vec<Box<PostEval>>,
}

#[cfg(feature = "pre-eval")]
impl BoxedHooks {
    pub fn new(pre_eval: PreEval) -> Self {
        BoxedHooks {
            pre_eval,
            posteval_stack: Vec::new(),
        }
    }
}

#[cfg(feature = "pre-eval")]
impl EvalHooks for BoxedHooks {
    fn pre_eval(
        &mut self,
        allocator: &mut Allocator,
        program: NodePtr,
        env: NodePtr,
    ) -> Result<bool> {
        let Some(post_eval) = (self.pre_eval)(allocator, program, env)? else {
            return Ok(false);
        };
        self.posteval_stack.push(post_eval);
        Ok(true)
    }

    fn post_eval(&mut self, allocator: &mut Allocator, result: Option<NodePtr>) {
        let f = self.posteval_stack.pop().unwrap();
        f(allocator, result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_hooks_schedules_nothing() {
        let mut a = Allocator::new();
        let nil = a.nil();
        assert!(!NoHooks.pre_eval(&mut a, nil, nil).unwrap());
        NoHooks.post_eval(&mut a, Some(nil));
    }

    #[cfg(feature = "pre-eval")]
    #[test]
    fn test_boxed_pre_eval_returning_none() {
        use crate::chia_dialect::{ChiaDialect, ClvmFlags};
        use crate::reduction::Reduction;
        use crate::run_program::{run_program, run_program_with_pre_eval};
        use crate::test_ops::{node_eq, parse_exp};
        use std::cell::Cell;
        use std::rc::Rc;

        let mut a = Allocator::new();
        let (program, rest) = parse_exp(&mut a, "(+ (q . 2) (q . 3))");
        assert_eq!(rest, "");
        let args = a.nil();
        let dialect = ChiaDialect::new(ClvmFlags::ENABLE_GC);

        let Reduction(expected_cost, expected) =
            run_program(&mut a, &dialect, program, args, 10000).unwrap();

        let calls = Rc::new(Cell::new(0));
        let pre_eval: PreEval = {
            let calls = Rc::clone(&calls);
            Box::new(move |_, _, _| {
                calls.set(calls.get() + 1);
                Ok(None)
            })
        };
        let Reduction(cost, result) =
            run_program_with_pre_eval(&mut a, &dialect, program, args, 10000, Some(pre_eval))
                .unwrap();

        assert_eq!(cost, expected_cost);
        assert!(node_eq(&a, result, expected));
        // called for the program and each of its two operands
        assert_eq!(calls.get(), 3);
    }
}
