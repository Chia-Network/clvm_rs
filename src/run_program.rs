use super::traverse_path::traverse_path;
use crate::allocator::{Allocator, Checkpoint, NodePtr, SExp};
use crate::cost::Cost;
use crate::dialect::{Dialect, OperatorSet};
use crate::err_utils::err;
use crate::op_utils::{atom, first, get_args, uint_atom};
use crate::reduction::{EvalErr, Reduction, Response};

// lowered from 46
const QUOTE_COST: Cost = 20;
// lowered from 138
const APPLY_COST: Cost = 90;
// the cost of entering a softfork guard
const GUARD_COST: Cost = 140;
// mandatory base cost for every operator we execute
const OP_COST: Cost = 1;

// The max number of elements allowed on the stack. The program fails if this is
// exceeded
const STACK_SIZE_LIMIT: usize = 20000000;

#[cfg(feature = "pre-eval")]
pub type PreEval =
    Box<dyn Fn(&mut Allocator, NodePtr, NodePtr) -> Result<Option<Box<PostEval>>, EvalErr>>;

#[cfg(feature = "pre-eval")]
pub type PostEval = dyn Fn(Option<NodePtr>);

#[repr(u8)]
enum Operation {
    Apply,
    Cons,
    ExitGuard,
    SwapEval,

    #[cfg(feature = "pre-eval")]
    PostEval,
}

#[cfg(feature = "counters")]
#[derive(Debug)]
pub struct Counters {
    pub val_stack_usage: usize,
    pub env_stack_usage: usize,
    pub op_stack_usage: usize,
    pub atom_count: u32,
    pub pair_count: u32,
    pub heap_size: u32,
}

#[cfg(feature = "counters")]
impl Counters {
    fn new() -> Self {
        Counters {
            val_stack_usage: 0,
            env_stack_usage: 0,
            op_stack_usage: 0,
            atom_count: 0,
            pair_count: 0,
            heap_size: 0,
        }
    }
}

// this represents the state we were in before entering a soft-fork guard. We
// may need this to long-jump out of the guard, and also to validate the cost
// when exiting the guard
struct SoftforkGuard {
    // This is the expected cost of the program when exiting the guard. i.e. the
    // current_cost + the first argument to the operator
    expected_cost: Cost,

    // When exiting a softfork guard, all values used inside it are zapped. This
    // was the state of the allocator before entering. We restore to this state
    // on exit.
    allocator_state: Checkpoint,

    // this specifies which new operators are available
    operator_set: OperatorSet,

    #[cfg(test)]
    start_cost: Cost,
}

// `run_program` has three stacks:
// 1. the operand stack of `NodePtr` objects. val_stack
// 2. the operator stack of Operation. op_stack
// 3. the environment stack (points to the environment for the current
//    operation). env_stack

struct RunProgramContext<'a, D> {
    allocator: &'a mut Allocator,
    dialect: &'a D,
    val_stack: Vec<NodePtr>,
    env_stack: Vec<NodePtr>,
    op_stack: Vec<Operation>,
    softfork_stack: Vec<SoftforkGuard>,
    #[cfg(feature = "counters")]
    pub counters: Counters,

    #[cfg(feature = "pre-eval")]
    pre_eval: Option<PreEval>,
    #[cfg(feature = "pre-eval")]
    posteval_stack: Vec<Box<PostEval>>,
}

fn augment_cost_errors(r: Result<Cost, EvalErr>, max_cost: NodePtr) -> Result<Cost, EvalErr> {
    r.map_err(|e| {
        if &e.1 != "cost exceeded" {
            e
        } else {
            EvalErr(max_cost, e.1)
        }
    })
}

impl<'a, D: Dialect> RunProgramContext<'a, D> {
    #[cfg(feature = "counters")]
    #[inline(always)]
    fn account_val_push(&mut self) {
        self.counters.val_stack_usage =
            std::cmp::max(self.counters.val_stack_usage, self.val_stack.len());
    }

    #[cfg(feature = "counters")]
    #[inline(always)]
    fn account_env_push(&mut self) {
        self.counters.env_stack_usage =
            std::cmp::max(self.counters.env_stack_usage, self.env_stack.len());
    }

    #[cfg(feature = "counters")]
    #[inline(always)]
    fn account_op_push(&mut self) {
        self.counters.op_stack_usage =
            std::cmp::max(self.counters.op_stack_usage, self.op_stack.len());
    }

    #[cfg(not(feature = "counters"))]
    #[inline(always)]
    fn account_val_push(&mut self) {}

    #[cfg(not(feature = "counters"))]
    #[inline(always)]
    fn account_env_push(&mut self) {}

    #[cfg(not(feature = "counters"))]
    #[inline(always)]
    fn account_op_push(&mut self) {}

    pub fn pop(&mut self) -> Result<NodePtr, EvalErr> {
        let v: Option<NodePtr> = self.val_stack.pop();
        match v {
            None => {
                let node: NodePtr = self.allocator.null();
                err(node, "runtime error: value stack empty")
            }
            Some(k) => Ok(k),
        }
    }
    pub fn push(&mut self, node: NodePtr) -> Result<(), EvalErr> {
        if self.val_stack.len() == STACK_SIZE_LIMIT {
            return err(node, "value stack limit reached");
        }
        self.val_stack.push(node);
        self.account_val_push();
        Ok(())
    }

    pub fn push_env(&mut self, env: NodePtr) -> Result<(), EvalErr> {
        if self.env_stack.len() == STACK_SIZE_LIMIT {
            return err(env, "environment stack limit reached");
        }
        self.env_stack.push(env);
        self.account_env_push();
        Ok(())
    }

    #[cfg(feature = "pre-eval")]
    fn new_with_pre_eval(
        allocator: &'a mut Allocator,
        dialect: &'a D,
        pre_eval: Option<PreEval>,
    ) -> Self {
        RunProgramContext {
            allocator,
            dialect,
            val_stack: Vec::new(),
            env_stack: Vec::new(),
            op_stack: Vec::new(),
            softfork_stack: Vec::new(),
            #[cfg(feature = "counters")]
            counters: Counters::new(),
            pre_eval,
            posteval_stack: Vec::new(),
        }
    }

    fn new(allocator: &'a mut Allocator, dialect: &'a D) -> Self {
        RunProgramContext {
            allocator,
            dialect,
            val_stack: Vec::new(),
            env_stack: Vec::new(),
            op_stack: Vec::new(),
            softfork_stack: Vec::new(),
            #[cfg(feature = "counters")]
            counters: Counters::new(),
            #[cfg(feature = "pre-eval")]
            pre_eval: None,
            #[cfg(feature = "pre-eval")]
            posteval_stack: Vec::new(),
        }
    }

    fn cons_op(&mut self) -> Result<Cost, EvalErr> {
        /* Join the top two operands. */
        let v1 = self.pop()?;
        let v2 = self.pop()?;
        let p = self.allocator.new_pair(v1, v2)?;
        self.push(p)?;
        Ok(0)
    }

    fn eval_op_atom(
        &mut self,
        operator_node: NodePtr,
        operand_list: NodePtr,
        env: NodePtr,
    ) -> Result<Cost, EvalErr> {
        let op_atom = self.allocator.atom(operator_node);
        // special case check for quote
        if op_atom == self.dialect.quote_kw() {
            self.push(operand_list)?;
            Ok(QUOTE_COST)
        } else {
            self.push_env(env)?;
            self.op_stack.push(Operation::Apply);
            self.account_op_push();
            self.push(operator_node)?;
            let mut operands: NodePtr = operand_list;
            while let SExp::Pair(first, rest) = self.allocator.sexp(operands) {
                // We evaluate every entry in the argument list (using the
                // environment at the top of the env_stack) The resulting return
                // values are arranged in a list. the top item on the stack is
                // the resulting list, and below it is the next pair to
                // evaluated.
                //
                // each evaluation pops both, pushes the result list
                // back, evaluates and the executes the Cons operation
                // to add the most recent result to the list. Leaving
                // the new list at the top of the stack for the next
                // pair to be evaluated.
                self.op_stack.push(Operation::SwapEval);
                self.account_op_push();
                self.push(first)?;
                operands = rest;
            }
            // ensure a correct null terminator
            if !self.allocator.atom(operands).is_empty() {
                err(operand_list, "bad operand list")
            } else {
                self.push(self.allocator.null())?;
                Ok(OP_COST)
            }
        }
    }

    fn eval_pair(&mut self, program: NodePtr, env: NodePtr) -> Result<Cost, EvalErr> {
        #[cfg(feature = "pre-eval")]
        if let Some(pre_eval) = &self.pre_eval {
            if let Some(post_eval) = pre_eval(self.allocator, program, env)? {
                self.posteval_stack.push(post_eval);
                self.op_stack.push(Operation::PostEval);
            }
        };

        // put a bunch of ops on op_stack
        let (op_node, op_list) = match self.allocator.sexp(program) {
            // the program is just a bitfield path through the env tree
            SExp::Atom => {
                let r: Reduction =
                    traverse_path(self.allocator, self.allocator.atom(program), env)?;
                self.push(r.1)?;
                return Ok(r.0);
            }
            // the program is an operator and a list of operands
            SExp::Pair(operator_node, operand_list) => (operator_node, operand_list),
        };

        match self.allocator.sexp(op_node) {
            SExp::Pair(new_operator, _) => {
                let [inner] = get_args::<1>(
                    self.allocator,
                    op_node,
                    "in the ((X)...) syntax, the inner list",
                )?;
                if let SExp::Pair(_, _) = self.allocator.sexp(inner) {
                    return err(program, "in ((X)...) syntax X must be lone atom");
                }
                self.push_env(env)?;
                self.push(new_operator)?;
                self.push(op_list)?;
                self.op_stack.push(Operation::Apply);
                self.account_op_push();
                Ok(APPLY_COST)
            }
            SExp::Atom => self.eval_op_atom(op_node, op_list, env),
        }
    }

    fn swap_eval_op(&mut self) -> Result<Cost, EvalErr> {
        let v2 = self.pop()?;
        let program: NodePtr = self.pop()?;
        let env: NodePtr = *self
            .env_stack
            .last()
            .ok_or_else(|| EvalErr(program, "runtime error: env stack empty".into()))?;
        self.push(v2)?;

        // on the way back, build a list from the values
        self.op_stack.push(Operation::Cons);
        self.account_op_push();

        self.eval_pair(program, env)
    }

    fn parse_softfork_arguments(
        &self,
        args: NodePtr,
    ) -> Result<(OperatorSet, NodePtr, NodePtr), EvalErr> {
        let [_cost, extension, program, env] = get_args::<4>(self.allocator, args, "softfork")?;

        let extension =
            self.dialect
                .softfork_extension(uint_atom::<4>(self.allocator, extension, "softfork")? as u32);
        if extension == OperatorSet::Default {
            err(args, "unknown softfork extension")
        } else {
            Ok((extension, program, env))
        }
    }

    fn apply_op(&mut self, current_cost: Cost, max_cost: Cost) -> Result<Cost, EvalErr> {
        let operand_list = self.pop()?;
        let operator = self.pop()?;
        let op_atom = atom(self.allocator, operator, "(internal error) apply")?;
        if self.env_stack.pop().is_none() {
            return err(operator, "runtime error: env stack empty");
        }
        if op_atom == self.dialect.apply_kw() {
            let [new_operator, env] = get_args::<2>(self.allocator, operand_list, "apply")?;
            self.eval_pair(new_operator, env).map(|c| c + APPLY_COST)
        } else if op_atom == self.dialect.softfork_kw() {
            let expected_cost = uint_atom::<8>(
                self.allocator,
                first(self.allocator, operand_list)?,
                "softfork",
            )?;
            if expected_cost > max_cost {
                return err(operand_list, "cost exceeded");
            }
            if expected_cost == 0 {
                return err(operand_list, "cost must be > 0");
            }

            // we can't blindly propagate errors here, since we handle errors
            // differently depending on whether we allow unknown ops or not
            let (ext, prg, env) = match self.parse_softfork_arguments(operand_list) {
                Ok(ret_values) => ret_values,
                Err(err) => {
                    if self.dialect.allow_unknown_ops() {
                        // In this case, we encountered a softfork invocation
                        // that doesn't pass the correct arguments.
                        // if we're in consensus mode, we have to accept this as
                        // something we don't understand
                        self.push(self.allocator.null())?;
                        return Ok(expected_cost);
                    }
                    return Err(err);
                }
            };

            self.softfork_stack.push(SoftforkGuard {
                expected_cost: current_cost + expected_cost,
                allocator_state: self.allocator.checkpoint(),
                operator_set: ext,
                #[cfg(test)]
                start_cost: current_cost,
            });

            // once the softfork guard exits, we need to ensure the cost that was
            // specified match the true cost. We also free heap allocations
            self.op_stack.push(Operation::ExitGuard);

            self.eval_pair(prg, env).map(|c| c + GUARD_COST)
        } else {
            let current_extensions = if let Some(sf) = self.softfork_stack.last() {
                sf.operator_set
            } else {
                OperatorSet::Default
            };

            let r = self.dialect.op(
                self.allocator,
                operator,
                operand_list,
                max_cost,
                current_extensions,
            )?;
            self.push(r.1)?;
            Ok(r.0)
        }
    }

    fn exit_guard(&mut self, current_cost: Cost) -> Result<Cost, EvalErr> {
        // this is called when we are done executing a softfork program.
        // This is when we have to validate the cost
        let guard = self
            .softfork_stack
            .pop()
            .expect("internal error. exiting a softfork that's already been popped");

        if current_cost != guard.expected_cost {
            #[cfg(test)]
            println!(
                "actual cost: {} specified cost: {}",
                current_cost - guard.start_cost,
                guard.expected_cost - guard.start_cost
            );
            return err(self.allocator.null(), "softfork specified cost mismatch");
        }

        // restore the allocator to the state when we entered the softfork guard
        // This is an optimization to reclaim all heap space allocated by the
        // softfork program. Since the softfork always return null, no value can
        // escape the softfork program, and it's therefore safe to restore the
        // heap
        self.allocator.restore_checkpoint(&guard.allocator_state);

        // the softfork always returns null, pop the value pushed by the
        // evaluation of the program and push null instead
        self.pop()
            .expect("internal error, softfork program did not push value onto stack");

        self.push(self.allocator.null())?;

        Ok(0)
    }

    pub fn run_program(&mut self, program: NodePtr, env: NodePtr, max_cost: Cost) -> Response {
        self.val_stack = vec![];
        self.op_stack = vec![];

        // max_cost is always in effect, and necessary to prevent wrap-around of
        // the cost integer.
        let max_cost = if max_cost == 0 { Cost::MAX } else { max_cost };
        let max_cost_ptr = self.allocator.new_number(max_cost.into())?;

        let mut cost: Cost = 0;

        cost += self.eval_pair(program, env)?;

        loop {
            // if we are in a softfork guard, temporarily use the guard's
            // expected cost as the upper limit. This lets us fail early in case
            // it's wrong. It's guaranteed to be <= max_cost, because we check
            // that when entering the softfork guard
            let effective_max_cost = if let Some(sf) = self.softfork_stack.last() {
                sf.expected_cost
            } else {
                max_cost
            };

            if cost > effective_max_cost {
                return err(max_cost_ptr, "cost exceeded");
            }
            let top = self.op_stack.pop();
            let op = match top {
                Some(f) => f,
                None => break,
            };
            cost += match op {
                Operation::Apply => augment_cost_errors(
                    self.apply_op(cost, effective_max_cost - cost),
                    max_cost_ptr,
                )?,
                Operation::ExitGuard => self.exit_guard(cost)?,
                Operation::Cons => self.cons_op()?,
                Operation::SwapEval => augment_cost_errors(self.swap_eval_op(), max_cost_ptr)?,
                #[cfg(feature = "pre-eval")]
                Operation::PostEval => {
                    let f = self.posteval_stack.pop().unwrap();
                    let peek: Option<NodePtr> = self.val_stack.last().copied();
                    f(peek);
                    0
                }
            };
        }
        Ok(Reduction(cost, self.pop()?))
    }
}

pub fn run_program<'a, D: Dialect>(
    allocator: &'a mut Allocator,
    dialect: &'a D,
    program: NodePtr,
    env: NodePtr,
    max_cost: Cost,
) -> Response {
    let mut rpc = RunProgramContext::new(allocator, dialect);
    rpc.run_program(program, env, max_cost)
}

#[cfg(feature = "pre-eval")]
pub fn run_program_with_pre_eval<'a, D: Dialect>(
    allocator: &'a mut Allocator,
    dialect: &'a D,
    program: NodePtr,
    env: NodePtr,
    max_cost: Cost,
    pre_eval: Option<PreEval>,
) -> Response {
    let mut rpc = RunProgramContext::new_with_pre_eval(allocator, dialect, pre_eval);
    rpc.run_program(program, env, max_cost)
}

#[cfg(feature = "counters")]
pub fn run_program_with_counters<'a, D: Dialect>(
    allocator: &'a mut Allocator,
    dialect: &'a D,
    program: NodePtr,
    env: NodePtr,
    max_cost: Cost,
) -> (Counters, Response) {
    let mut rpc = RunProgramContext::new(allocator, dialect);
    let ret = rpc.run_program(program, env, max_cost);
    rpc.counters.atom_count = rpc.allocator.atom_count() as u32;
    rpc.counters.pair_count = rpc.allocator.pair_count() as u32;
    rpc.counters.heap_size = rpc.allocator.heap_size() as u32;
    (rpc.counters, ret)
}

#[cfg(test)]
struct RunProgramTest {
    prg: &'static str,
    args: &'static str,
    flags: u32,
    result: Option<&'static str>,
    cost: Cost,
    err: &'static str,
}

#[cfg(test)]
use crate::test_ops::parse_exp;

#[cfg(test)]
use crate::chia_dialect::{ENABLE_BLS_OPS_OUTSIDE_GUARD, ENABLE_FIXED_DIV, NO_UNKNOWN_OPS};

#[cfg(test)]
const TEST_CASES: &[RunProgramTest] = &[
    RunProgramTest {
        prg: "(/ (q . 10) (q . -3))",
        args: "()",
        flags: 0,
        result: None,
        cost: 0,
        err: "div operator with negative operands is deprecated",
    },
    RunProgramTest {
        prg: "(/ (q . -10) (q . 3))",
        args: "()",
        flags: 0,
        result: None,
        cost: 0,
        err: "div operator with negative operands is deprecated",
    },
    RunProgramTest {
        prg: "(/ (q . 10) (q . -3))",
        args: "()",
        flags: ENABLE_FIXED_DIV,
        result: Some("-4"),
        cost: 1047,
        err: "",
    },
    RunProgramTest {
        prg: "(/ (q . -10) (q . 3))",
        args: "()",
        flags: ENABLE_FIXED_DIV,
        result: Some("-4"),
        cost: 1047,
        err: "",
    },
    RunProgramTest {
        prg: "(/ (q . -1) (q . 2))",
        args: "()",
        flags: ENABLE_FIXED_DIV,
        result: Some("-1"),
        cost: 1047,
        err: "",
    },
    // (mod (X N) (defun power (X N) (if (= N 0) 1 (* X (power X (- N 1))))) (power X N))
    RunProgramTest {
        prg: "(a (q 2 2 (c 2 (c 5 (c 11 ())))) (c (q 2 (i (= 11 ()) (q 1 . 1) (q 18 5 (a 2 (c 2 (c 5 (c (- 11 (q . 1)) ())))))) 1) 1))",
        args: "(5033 1000)",
        flags: 0,
        result: Some("0x024d4f505f1f813ca5e0ae8805bad8707347e65c5f7595da4852be5074288431d1df11a0c326d249f1f52ee051579403d1d0c23a7a1e9af18b7d7dc4c63c73542863c434ae9dfa80141a30cf4acee0d6c896aa2e64ea748404427a3bdaa1b97e4e09b8f5e4f8e9c568a4fc219532dbbad5ec54476d19b7408f8e7e7df16b830c20a1e83d90cc0620b0677b7606307f725539ef223561cdb276baf8e92156ee6492d97159c8f64768349ea7e219fd07fa818a59d81d0563b140396402f0ff758840da19808440e0a57c94c48ef84b4ab7ca8c5f010b69b8f443b12b50bd91bdcf2a96208ddac283fa294d6a99f369d57ab41d03eab5bb4809223c141ad94378516e6766a5054e22e997e260978af68a86893890d612f081b40d54fd1e940af35c0d7900c9a917e2458a61ef8a83f7211f519b2c5f015dfa7c2949ef8bedd02d3bad64ca9b2963dc2bb79f24092331133a7a299872079b9d0422b8fc0eeba4e12c7667ac7282cc6ff98a7c670614c9fce5a061b8d5cd4dd3c6d62d245688b62f9713dc2604bdd5bbc85c070c51f784a9ebac0e0eaa2e29e82d93e570887aa7e1a9d25baf0b2c55a4615f35ec0dbe9baa921569700f95e10cd2d4f6ba152a2ac288c37b60980df33dadfa920fd43dbbf55a0b333b88a3237d954e33d80ed6582019faf51db5f1b52e392559323f8bdd945e7fc6cb8f97f2b8417cfc184d7bfbfa5314d4114f95b725847523f1848d13c28ad96662298ee4e2d87af23e7cb4e58d7a20a5c57ae6833b4a37dcafccca0245a0d6ef28f83200d74db390281e03dd3a8b782970895764c3fcef31c5ed6d0b6e4e796a62ad5654691eea0d9db351cc4fee63248405b24c98bd5e68e4a5e0ab11e90e3c7de270c594d3a35639d931853b7010c8c896f6b28b2af719e53da65da89d44b926b6f06123c9217a43be35d751516bd02c18c4f868a2eae78ae3c6deab1115086c8ce58414db4561865d17ab95c7b3d4e1bfc6d0a4d3fbf5f20a0a7d77a9270e4da354c588da55b0063aec76654019ffb310e1503d99a7bc81ccdf5f8b15c8638156038624cf35988d8420bfdb59184c4b86bf5448df65c44aedc2e98eead7f1ba4be8f402baf12d41076b8f0991cfc778e04ba2c05d1440c70488ffaeefde537064035037f729b683e8ff1b3d0b4aa26a2b30bcaa9379f7fcc7072ff9a2c3e801c5979b0ab3e7acf89373de642d596f26514b9fa213ca217181a8429ad69d14445a822b16818c2509480576dc0ff7bac48c557e6d1883039f4daf873fa4f9a4d849130e2e4336049cfaf9e69a7664f0202b901cf07c7065c4dc93c46f98c5ea5c9c9d911b733093490da3bf1c95f43cd18b7be3798535a55ac6da3442946a268b74bde1349ca9807c41d90c7ec218a17efd2c21d5fcd720501f8a488f1dfba0a423dfdb2a877707b77930e80d734ceabcdb24513fad8f2e2470604d041df083bf184edd0e9720dd2b608b1ee1df951d7ce8ec671317b4f5a3946aa75280658b4ef77b3f504ce73e7ecac84eec3c2b45fb62f6fbd5ab78c744abd3bf5d0ab37d7b19124d2470d53db09ddc1f9dd9654b0e6a3a44c95d0a5a5e061bd24813508d3d1c901544dc3e6b84ca38dd2fde5ea60a57cbc12428848c4e3f6fd4941ebd23d709a717a090dd01830436659f7c20fd2d70c916427e9f3f12ac479128c2783f02a9824aa4e31de133c2704e049a50160f656e28aa0a2615b32bd48bb5d5d13d363a487324c1e9b8703be938bc545654465c9282ad5420978263b3e3ba1bb45e1a382554ac68e5a154b896c9c4c2c3853fbbfc877c4fb7dc164cc420f835c413839481b1d2913a68d206e711fb19b284a7bb2bd2033531647cf135833a0f3026b0c1dc0c184120d30ef4865985fdacdfb848ab963d2ae26a784b7b6a64fdb8feacf94febed72dcd0a41dc12be26ed79af88f1d9cba36ed1f95f2da8e6194800469091d2dfc7b04cfe93ab7a7a888b2695bca45a76a1458d08c3b6176ab89e7edc56c7e01142adfff944641b89cd5703a911145ac4ec42164d90b6fcd78b39602398edcd1f935485894fb8a1f416e031624806f02fbd07f398dbfdd48b86dfacf2045f85ecfe5bb1f01fae758dcdb4ae3b1e2aac6f0878f700d1f430b8ca47c9d8254059bd5c006042c4605f33ca98b41"),
        cost: 15073165,
        err: "",
    },
    // '
    RunProgramTest {
        prg: "(= (point_add (pubkey_for_exp (q . -2)) (pubkey_for_exp (q . 5))) (pubkey_for_exp (q . 3)))",
        args: "()",
        flags: 0,
        result: Some("1"),
        cost: 6768556,
        err: "",
    },
    RunProgramTest {
        prg: "(= (point_add (pubkey_for_exp (q . 2)) (pubkey_for_exp (q . 3))) (pubkey_for_exp (q . 5)))",
        args: "()",
        flags: 0,
        result: Some("1"),
        cost: 6768556,
        err: "",
    },
    RunProgramTest {
        prg: "(point_add (pubkey_for_exp (q . 1)) (pubkey_for_exp (q . 2)))",
        args: "()",
        flags: 0,
        result: Some("0x89ece308f9d1f0131765212deca99697b112d61f9be9a5f1f3780a51335b3ff981747a0b2ca2179b96d2c0c9024e5224"),
        cost: 5442073,
        err: "",
    },
    RunProgramTest {
        prg: "(f (f (q . ((100 200 300) 400 500))))",
        args: "()",
        flags: 0,
        result: Some("0x64"),
        cost: 82,
        err: "",
    },
    RunProgramTest {
        prg: "(= (f 1) (+ (f (r 1)) (f (r (r 1)))))",
        args: "(7 3 3)",
        flags: 0,
        result: Some("()"),
        cost: 1194,
        err: "",
    },
    RunProgramTest {
        prg: "(= (f 1) (+ (f (r 1)) (f (r (r 1)))))",
        args: "(7 3 4)",
        flags: 0,
        result: Some("1"),
        cost: 1194,
        err: "",
    },
    RunProgramTest {
        prg: "(i (f (r (r 1))) (f 1) (f (r 1)))",
        args: "(200 300 400)",
        flags: 0,
        result: Some("0x00c8"),
        cost: 352,
        err: "",
    },
    RunProgramTest {
        prg: "(i (f (r (r 1))) (f 1) (f (r 1)))",
        args: "(200 300 1)",
        flags: 0,
        result: Some("0x00c8"),
        cost: 352,
        err: "",
    },
    RunProgramTest {
        prg: "(r (r (q . ((100 200 300) 400 500))))",
        args: "()",
        flags: 0,
        result: Some("(500)"),
        cost: 82,
        err: "",
    },
    RunProgramTest {
        prg: "(* (q . 10000000000000000000000000000000000) (q . 10000000000000000000000000000000) (q . 100000000000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000))",
        args: "()",
        flags: 0,
        result: Some("0x04261a5c969abab851babdb4f178e63bf2ed3879fc13a4c75622d73c909440a4763849b52e49cd2522500f555f6a3131775f93ddcf24eda7a1dbdf828a033626da873caaaa880a9121f4c44a157973f60443dc53bc99ac12d5bd5fa20a88320ae2ccb8e1b5e792cbf0d001bb0fbd7765d3936e412e2fc8f1267833237237fcb638dda0a7aa674680000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"),
        cost: 24255,
        err: "",
    },

    // ## APPLY
    RunProgramTest {
        prg: "(a (q 0x0fffffffff) (q ()))",
        args: "()",
        flags: 0,
        result: None,
        cost: 0,
        err: "invalid operator",
    },
    RunProgramTest {
        prg: "(a (q . 0) (q . 1) (q . 2))",
        args: "()",
        flags: 0,
        result: None,
        cost: 0,
        err: "apply takes exactly 2 arguments",
    },
    RunProgramTest {
        prg: "(a (q 0x00ffffffffffffffffffff00) (q ()))",
        args: "()",
        flags: 0,
        result: None,
        cost: 0,
        err: "invalid operator",
    },
    RunProgramTest {
        prg: "(a (q . 1))",
        args: "()",
        flags: 0,
        result: None,
        cost: 0,
        err: "apply takes exactly 2 arguments",
    },
    RunProgramTest {
        prg: "(a (q . 1) (q . (100 200)))",
        args: "()",
        flags: 0,
        result: Some("(100 200)"),
        cost: 175,
        err: "",
    },
    RunProgramTest {
        prg: "(a (q . (+ 2 5)) (q . (20 30)))",
        args: "()",
        flags: 0,
        result: Some("50"),
        cost: 987,
        err: "",
    },
    RunProgramTest {
        prg: "((c (q . (+ (q . 50) 1)) (q . 500)))",
        args: "()",
        flags: 0,
        result: None,
        cost: 0,
        err: "in the ((X)...) syntax, the inner list takes exactly 1 argument",
    },
    RunProgramTest {
        prg: "((#c) (q . 3) (q . 4))",
        args: "()",
        flags: 0,
        result: Some("((1 . 3) 1 . 4)"),
        cost: 140,
        err: "",
    },
    RunProgramTest {
        prg: "((#+) 1 2 3)",
        args: "()",
        flags: 0,
        result: Some("6"),
        cost: 1168,
        err: "",
    },
    RunProgramTest {
        prg: "(a (q . 2) (q . (3 4 5)))",
        args: "()",
        flags: 0,
        result: Some("3"),
        cost: 179,
        err: "",
    },

    // ## PATH LOOKUPS

    // 0
    RunProgramTest {
        prg: "0",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("()"),
        cost: 44,
        err: "",
    },
    // 1
    RunProgramTest {
        prg: "1",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("(((8 . 12) 10 . 14) (9 . 13) 11 . 15)"),
        cost: 44,
        err: "",
    },
    // 2
    RunProgramTest {
        prg: "2",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("((8 . 12) 10 . 14)"),
        cost: 48,
        err: "",
    },
    // 3
    RunProgramTest {
        prg: "3",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("((9 . 13) 11 . 15)"),
        cost: 48,
        err: "",
    },
    // 4
    RunProgramTest {
        prg: "4",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("(8 . 12)"),
        cost: 52,
        err: "",
    },
    // 5
    RunProgramTest {
        prg: "5",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("(9 . 13)"),
        cost: 52,
        err: "",
    },
    // 6
    RunProgramTest {
        prg: "6",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("(10 . 14)"),
        cost: 52,
        err: "",
    },
    // 7
    RunProgramTest {
        prg: "7",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("(11 . 15)"),
        cost: 52,
        err: "",
    },
    RunProgramTest {
        prg: "8",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("8"),
        cost: 56,
        err: "",
    },
    RunProgramTest {
        prg: "9",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("9"),
        cost: 56,
        err: "",
    },
    RunProgramTest {
        prg: "10",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("10"),
        cost: 56,
        err: "",
    },
    RunProgramTest {
        prg: "11",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("11"),
        cost: 56,
        err: "",
    },
    RunProgramTest {
        prg: "12",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("12"),
        cost: 56,
        err: "",
    },
    RunProgramTest {
        prg: "13",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("13"),
        cost: 56,
        err: "",
    },
    RunProgramTest {
        prg: "14",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("14"),
        cost: 56,
        err: "",
    },
    RunProgramTest {
        prg: "15",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        flags: 0,
        result: Some("15"),
        cost: 56,
        err: "",
    },
    RunProgramTest {
        prg: "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001",
        args: "(((0x1337 . (0x1337 . (42 . 0x1337))) . 0x1337) . 0x1337)",
        flags: 0,
        result: Some("(((0x1337 . (0x1337 . (42 . 0x1337))) . 0x1337) . 0x1337)"),
        cost: 536,
        err: "",
    },
    RunProgramTest {
        prg: "0x0000C8C141AB3121E776",
        args: "((0x1337 . (0x1337 . ((0x1337 . (0x1337 . (0x1337 . ((0x1337 . (0x1337 . (0x1337 . (((0x1337 . (0x1337 . (0x1337 . (0x1337 . (((((0x1337 . (((0x1337 . ((((0x1337 . (0x1337 . (((0x1337 . (0x1337 . ((0x1337 . ((0x1337 . ((0x1337 . (0x1337 . ((((((0x1337 . ((0x1337 . ((((((0x1337 . (0x1337 . ((((0x1337 . (((0x1337 . 42) . 0x1337) . 0x1337)) . 0x1337) . 0x1337) . 0x1337))) . 0x1337) . 0x1337) . 0x1337) . 0x1337) . 0x1337)) . 0x1337)) . 0x1337) . 0x1337) . 0x1337) . 0x1337) . 0x1337))) . 0x1337)) . 0x1337)) . 0x1337))) . 0x1337) . 0x1337))) . 0x1337) . 0x1337) . 0x1337)) . 0x1337) . 0x1337)) . 0x1337) . 0x1337) . 0x1337) . 0x1337))))) . 0x1337) . 0x1337)))) . 0x1337)))) . 0x1337))) . 0x1337)",
        flags: 0,
        result: Some("42"),
        cost: 304,
        err: "",
    },
    RunProgramTest {
        prg: "7708975405620101644641102810267383005",
        args: "(0x1337 . ((0x1337 . (0x1337 . (0x1337 . ((0x1337 . (0x1337 . (((0x1337 . ((0x1337 . (0x1337 . (0x1337 . (0x1337 . (0x1337 . ((0x1337 . (0x1337 . ((0x1337 . (((0x1337 . (0x1337 . (0x1337 . ((0x1337 . (((0x1337 . (((0x1337 . (0x1337 . (0x1337 . (0x1337 . ((0x1337 . ((0x1337 . (((((0x1337 . ((0x1337 . ((0x1337 . (0x1337 . (0x1337 . (((0x1337 . (0x1337 . ((0x1337 . (0x1337 . ((((0x1337 . (0x1337 . (0x1337 . (0x1337 . (((((0x1337 . (0x1337 . (0x1337 . (0x1337 . (0x1337 . (((((0x1337 . (((((0x1337 . ((0x1337 . (0x1337 . ((((0x1337 . ((((0x1337 . ((0x1337 . ((0x1337 . ((0x1337 . (0x1337 . (0x1337 . ((((0x1337 . (0x1337 . ((0x1337 . (((0x1337 . (0x1337 . (((0x1337 . (0x1337 . (0x1337 . (42 . 0x1337)))) . 0x1337) . 0x1337))) . 0x1337) . 0x1337)) . 0x1337))) . 0x1337) . 0x1337) . 0x1337)))) . 0x1337)) . 0x1337)) . 0x1337)) . 0x1337) . 0x1337) . 0x1337)) . 0x1337) . 0x1337) . 0x1337))) . 0x1337)) . 0x1337) . 0x1337) . 0x1337) . 0x1337)) . 0x1337) . 0x1337) . 0x1337) . 0x1337)))))) . 0x1337) . 0x1337) . 0x1337) . 0x1337))))) . 0x1337) . 0x1337) . 0x1337))) . 0x1337))) . 0x1337) . 0x1337)))) . 0x1337)) . 0x1337)) . 0x1337) . 0x1337) . 0x1337) . 0x1337)) . 0x1337)) . 0x1337))))) . 0x1337) . 0x1337)) . 0x1337) . 0x1337)) . 0x1337)))) . 0x1337) . 0x1337)) . 0x1337))) . 0x1337)))))) . 0x1337)) . 0x1337) . 0x1337))) . 0x1337)))) . 0x1337))",
        flags: 0,
        result: Some("42"),
        cost: 532,
        err: "",
    },
    RunProgramTest {
        prg: "1",
        args: "1",
        flags: 0,
        result: Some("1"),
        cost: 44,
        err: "",
    },
    RunProgramTest {
        prg: "(> 3 3)",
        args: "()",
        flags: 0,
        result: None,
        cost: 0,
        err: "path into atom",
    },

    // ## SOFTFORK

    // the arguments to softfork are checked in mempool mode, but in consensus
    // mode, only the cost argument is
    RunProgramTest {
        prg: "(softfork (q . 979))",
        args: "()",
        flags: 0,
        result: Some("()"),
        cost: 1000,
        err: "",
    },
    RunProgramTest {
        prg: "(softfork (q . 979))",
        args: "()",
        flags: NO_UNKNOWN_OPS,
        result: None,
        cost: 1000,
        err: "softfork takes exactly 4 arguments",
    },
    RunProgramTest {
        prg: "(softfork (q . 959) (q . 9))",
        args: "()",
        flags: 0,
        result: Some("()"),
        cost: 1000,
        err: "",
    },
    RunProgramTest {
        prg: "(softfork (q . 959) (q . 9))",
        args: "()",
        flags: NO_UNKNOWN_OPS,
        result: None,
        cost: 1000,
        err: "softfork takes exactly 4 arguments",
    },
    RunProgramTest {
        prg: "(softfork (q . 939) (q . 9) (q x))",
        args: "()",
        flags: 0,
        result: Some("()"),
        cost: 1000,
        err: "",
    },
    RunProgramTest {
        prg: "(softfork (q . 939) (q . 9) (q x))",
        args: "()",
        flags: NO_UNKNOWN_OPS,
        result: None,
        cost: 1000,
        err: "softfork takes exactly 4 arguments",
    },
    // this is a valid invocation, but we don't implement any extensions (yet)
    // so the extension specifier 0 is still unknown
    RunProgramTest {
        prg: "(softfork (q . 919) (q . 9) (q x) (q . ()))",
        args: "()",
        flags: 0,
        result: Some("()"),
        cost: 1000,
        err: "",
    },
    // when parsing the cost argument, we ignore redundant leading zeroes
    RunProgramTest {
        prg: "(softfork (q . 0x00000397) (q . 9) (q x) (q . ()))",
        args: "()",
        flags: 0,
        result: Some("()"),
        cost: 1000,
        err: "",
    },
    RunProgramTest {
        prg: "(softfork (q . 919) (q . 9) (q x) (q . ()))",
        args: "()",
        flags: NO_UNKNOWN_OPS,
        result: None,
        cost: 1000,
        err: "unknown softfork extension",
    },

    // this is a valid invocation, but we don't implement any extensions (yet)
    RunProgramTest {
        prg: "(softfork (q . 919) (q . 0x00ffffffff) (q x) (q . ()))",
        args: "()",
        flags: 0,
        result: Some("()"),
        cost: 1000,
        err: "",
    },
    RunProgramTest {
        prg: "(softfork (q . 919) (q . 0x00ffffffff) (q x) (q . ()))",
        args: "()",
        flags: NO_UNKNOWN_OPS,
        result: None,
        cost: 1000,
        err: "unknown softfork extension",
    },

    // we don't allow negative "extension" parameters
    RunProgramTest {
        prg: "(softfork (q . 919) (q . -1) (q x) (q . ()))",
        args: "()",
        flags: 0,
        result: Some("()"),
        cost: 1000,
        err: "",
    },
    RunProgramTest {
        prg: "(softfork (q . 919) (q . -1) (q x) (q . ()))",
        args: "()",
        flags: NO_UNKNOWN_OPS,
        result: None,
        cost: 1000,
        err: "softfork requires positive int arg",
    },

    // we don't allow "extension" parameters > u32::MAX
    RunProgramTest {
        prg: "(softfork (q . 919) (q . 0x0100000000) (q x) (q . ()))",
        args: "()",
        flags: 0,
        result: Some("()"),
        cost: 1000,
        err: "",
    },
    RunProgramTest {
        prg: "(softfork (q . 919) (q . 0x0100000000) (q x) (q . ()))",
        args: "()",
        flags: NO_UNKNOWN_OPS,
        result: None,
        cost: 1000,
        err: "softfork requires u32 arg",
    },

    // we don't allow pairs as extension specifier
    RunProgramTest {
        prg: "(softfork (q . 919) (q 1 2 3) (q x) (q . ()))",
        args: "()",
        flags: 0,
        result: Some("()"),
        cost: 1000,
        err: "",
    },
    RunProgramTest {
        prg: "(softfork (q . 919) (q 1 2 3) (q x) (q . ()))",
        args: "()",
        flags: NO_UNKNOWN_OPS,
        result: None,
        cost: 1000,
        err: "softfork requires int arg",
    },

    // the cost value is checked in consensus mode as well
    RunProgramTest {
        prg: "(softfork (q . 1000))",
        args: "()",
        flags: 0,
        result: None,
        cost: 1000,
        err: "cost exceeded",
    },
    // the cost parameter is mandatory
    RunProgramTest {
        prg: "(softfork)",
        args: "()",
        flags: 0,
        result: None,
        cost: 0,
        err: "first of non-cons",
    },
    RunProgramTest {
        prg: "(softfork (q . 0))",
        args: "()",
        flags: 0,
        result: None,
        cost: 1000,
        err: "cost must be > 0",
    },
    // negative costs are not allowed
    RunProgramTest {
        prg: "(softfork (q . -1))",
        args: "()",
        flags: 0,
        result: None,
        cost: 1000,
        err: "softfork requires positive int arg",
    },
    RunProgramTest {
        prg: "(softfork (q 1 2 3))",
        args: "()",
        flags: 0,
        result: None,
        cost: 1000,
        err: "softfork requires int arg",
    },

    // test mismatching cost
    RunProgramTest {
        prg: "(softfork (q . 160) (q . 0) (q . (q . 42)) (q . ()))",
        args: "()",
        flags: 0,
        result: Some("()"),
        cost: 241,
        err: "",
    },
    // the program under the softfork is restricted by the specified cost
    RunProgramTest {
        prg: "(softfork (q . 159) (q . 0) (q . (q . 42)) (q . ()))",
        args: "()",
        flags: 0,
        result: None,
        cost: 241,
        err: "cost exceeded",
    },
    // the cost specified on the softfork must match exactly the cost of
    // executing the program
    RunProgramTest {
        prg: "(softfork (q . 161) (q . 0) (q . (q . 42)) (q . ()))",
        args: "()",
        flags: 0,
        result: None,
        cost: 10000,
        err: "softfork specified cost mismatch",
    },

    // without the flag to enable the BLS extensions, it's an unknown extension
    RunProgramTest {
        prg: "(softfork (q . 161) (q . 1) (q . (q . 42)) (q . ()))",
        args: "()",
        flags: NO_UNKNOWN_OPS,
        result: None,
        cost: 10000,
        err: "unknown softfork extension",
    },

    // coinid extension
    // make sure we can execute the coinid operator under softfork 0
    // this program raises an exception if the computed coin ID matches the
    // expected
    RunProgramTest {
        prg: "(softfork (q . 1432) (q . 0) (q a (i (= (coinid (q . 0x1234500000000000000000000000000000000000000000000000000000000000) (q . 0x6789abcdef000000000000000000000000000000000000000000000000000000) (q . 123456789)) (q . 0x69bfe81b052bfc6bd7f3fb9167fec61793175b897c16a35827f947d5cc98e4bc)) (q x) (q . 0)) (q . ())) (q . ()))",
        args: "()",
        flags: 0,
        result: None,
        cost: 1513,
        err: "clvm raise",
    },
    // also test the opposite. This program is the same as above but it raises
    // if the coin ID is a mismatch
    RunProgramTest {
        prg: "(softfork (q . 1432) (q . 0) (q a (i (= (coinid (q . 0x1234500000000000000000000000000000000000000000000000000000000000) (q . 0x6789abcdef000000000000000000000000000000000000000000000000000000) (q . 123456789)) (q . 0x69bfe81b052bfc6bd7f3fb9167fec61793175b897c16a35827f947d5cc98e4bc)) (q . 0) (q x)) (q . ())) (q . ()))",
        args: "()",
        flags: 0,
        result: Some("()"),
        cost: 1513,
        err: "",
    },

    // coinid operator after hardfork, where coinid is available outside the
    // softfork guard.
    RunProgramTest {
        prg: "(coinid (q . 0x1234500000000000000000000000000000000000000000000000000000000000) (q . 0x6789abcdef000000000000000000000000000000000000000000000000000000) (q . 123456789))",
        args: "()",
        flags: ENABLE_BLS_OPS_OUTSIDE_GUARD,
        result: Some("0x69bfe81b052bfc6bd7f3fb9167fec61793175b897c16a35827f947d5cc98e4bc"),
        cost: 861,
        err: "",
    },
    RunProgramTest {
        prg: "(coinid (q . 0x1234500000000000000000000000000000000000000000000000000000000000) (q . 0x6789abcdef000000000000000000000000000000000000000000000000000000) (q . 0x000123456789))",
        args: "()",
        flags: ENABLE_BLS_OPS_OUTSIDE_GUARD,
        result: None,
        cost: 861,
        err: "coinid: invalid amount (may not have redundant leading zero)",
    },
    // make sure the coinid operator is not available unless the flag is
    // specified
    RunProgramTest {
        prg: "(coinid (q . 0x1234500000000000000000000000000000000000000000000000000000000000) (q . 0x6789abcdef000000000000000000000000000000000000000000000000000000) (q . 0x000123456789))",
        args: "()",
        flags: NO_UNKNOWN_OPS,
        result: None,
        cost: 861,
        err: "unimplemented operator",
    },

    // secp261k1

    RunProgramTest {
        prg: "(secp256k1_verify (q . 0x02888b0c110ef0b4962e3fc6929cbba7a8bb25b4b2c885f55c76365018c909b439) (q . 0x74c2941eb2ebe5aa4f2287a4c5e506a6290c045004058de97a7edf0122548668) (q . 0x1acb7a6e062e78ccd4237b12c22f02b5a8d9b33cb3ba13c35e88e036baa1cbca75253bb9a96ffc48b43196c69c2972d8f965b1baa4e52348d8081cde65e6c018))",
        args: "()",
        flags: 0,
        result: Some("0"),
        cost: 1300061,
        err: "",
    },
    // invalid signature
    RunProgramTest {
        prg: "(secp256k1_verify (q . 0x02888b0c110ef0b4962e3fc6929cbba7a8bb25b4b2c885f55c76365018c909b439) (q . 0x74c2941eb2ebe5aa4f2287a4c5e506a6290c045004058de97a7edf0122548668) (q . 0x1acb7a6e062e78ccd4237b12c22f02b5a8d9b33cb3ba13c35e88e036baa1cbca75253bb9a96ffc48b43196c69c2972d8f965b1baa4e52348d8081cde65e6c019))",
        args: "()",
        flags: 0,
        result: None,
        cost: 0,
        err: "secp256k1_verify failed",
    },

    // secp261r1

    RunProgramTest {
        prg: "(secp256r1_verify (q . 0x0437a1674f3883b7171a11a20140eee014947b433723cf9f181a18fee4fcf96056103b3ff2318f00cca605e6f361d18ff0d2d6b817b1fa587e414f8bb1ab60d2b9) (q . 0x9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08) (q . 0xe8de121f4cceca12d97527cc957cca64a4bcfc685cffdee051b38ee81cb22d7e2c187fec82c731018ed2d56f08a4a5cbc40c5bfe9ae18c02295bb65e7f605ffc))",
        args: "()",
        flags: 0,
        result: Some("0"),
        cost: 1850061,
        err: "",
    },
    // invalid signature
    RunProgramTest {
        prg: "(secp256r1_verify (q . 0x0437a1674f3883b7171a11a20140eee014947b433723cf9f181a18fee4fcf96056103b3ff2318f00cca605e6f361d18ff0d2d6b817b1fa587e414f8bb1ab60d2b9) (q . 0x9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08) (q . 0xe8de121f4cceca12d97527cc957cca64a4bcfc685cffdee051b38ee81cb22d7e2c187fec82c731018ed2d56f08a4a5cbc40c5bfe9ae18c02295bb65e7f605ffd))",
        args: "()",
        flags: 0,
        result: None,
        cost: 0,
        err: "secp256r1_verify failed",
    },
];

#[cfg(test)]
fn check(res: (NodePtr, &str)) -> NodePtr {
    assert_eq!(res.1, "");
    res.0
}

#[test]
fn test_run_program() {
    use crate::chia_dialect::ChiaDialect;
    use crate::test_ops::node_eq;

    for t in TEST_CASES {
        let mut allocator = Allocator::new();

        let program = check(parse_exp(&mut allocator, t.prg));
        let args = check(parse_exp(&mut allocator, t.args));
        let expected_result = &t.result.map(|v| check(parse_exp(&mut allocator, v)));

        let dialect = ChiaDialect::new(t.flags);
        println!("prg: {}", t.prg);
        match run_program(&mut allocator, &dialect, program, args, t.cost) {
            Ok(Reduction(cost, prg_result)) => {
                assert!(node_eq(&allocator, prg_result, expected_result.unwrap()));
                assert_eq!(cost, t.cost);

                // now, run the same program again but with the cost limit 1 too low, to
                // ensure it fails with the correct error
                let expected_cost_exceeded =
                    run_program(&mut allocator, &dialect, program, args, t.cost - 1).unwrap_err();
                assert_eq!(expected_cost_exceeded.1, "cost exceeded");
            }
            Err(err) => {
                println!("FAILED: {}", err.1);
                assert_eq!(err.1, t.err);
                assert!(expected_result.is_none());
            }
        }
    }
}

#[cfg(feature = "counters")]
#[test]
fn test_counters() {
    use crate::chia_dialect::ChiaDialect;

    let mut a = Allocator::new();

    let program = check(parse_exp(&mut a, "(a (q 2 2 (c 2 (c 5 (c 11 ())))) (c (q 2 (i (= 11 ()) (q 1 . 1) (q 18 5 (a 2 (c 2 (c 5 (c (- 11 (q . 1)) ())))))) 1) 1))"));
    let args = check(parse_exp(&mut a, "(5033 1000)"));
    let cost = 15073165;

    let (counters, result) =
        run_program_with_counters(&mut a, &ChiaDialect::new(0), program, args, cost);

    assert_eq!(counters.val_stack_usage, 3015);
    assert_eq!(counters.env_stack_usage, 1005);
    assert_eq!(counters.op_stack_usage, 3014);
    assert_eq!(counters.atom_count, 2040);
    assert_eq!(counters.pair_count, 22077);
    assert_eq!(counters.heap_size, 771884);

    assert_eq!(result.unwrap().0, cost);
}
