use super::traverse_path::{traverse_path, traverse_path_fast};
use crate::allocator::{Allocator, Checkpoint, NodePtr, NodeVisitor, SExp};
use crate::cost::Cost;
use crate::dialect::{Dialect, OperatorSet};
use crate::error::{EvalErr, Result};
use crate::op_utils::{first, get_args, uint_atom};
use crate::reduction::{Reduction, Response};

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
pub type PreEval = Box<dyn Fn(&mut Allocator, NodePtr, NodePtr) -> Result<Option<Box<PostEval>>>>;

#[cfg(feature = "pre-eval")]
pub type PostEval = dyn Fn(&mut Allocator, Option<NodePtr>);

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
    pub small_atom_count: u32,
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
            small_atom_count: 0,
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

    pub fn pop(&mut self) -> Result<NodePtr> {
        let v: Option<NodePtr> = self.val_stack.pop();
        match v {
            None => Err(EvalErr::InternalError(
                NodePtr::NIL,
                "value stack empty".to_string(),
            ))?,
            Some(k) => Ok(k),
        }
    }
    pub fn push(&mut self, node: NodePtr) -> Result<()> {
        if self.val_stack.len() == STACK_SIZE_LIMIT {
            return Err(EvalErr::ValueStackLimitReached(node));
        }
        self.val_stack.push(node);
        self.account_val_push();
        Ok(())
    }

    pub fn push_env(&mut self, env: NodePtr) -> Result<()> {
        if self.env_stack.len() == STACK_SIZE_LIMIT {
            return Err(EvalErr::EnvironmentStackLimitReached(env));
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

    fn cons_op(&mut self) -> Result<Cost> {
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
    ) -> Result<Cost> {
        // special case check for quote
        if self.allocator.small_number(operator_node) == Some(self.dialect.quote_kw()) {
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
            // ensure a correct nil terminator
            if self.allocator.atom_len(operands) != 0 {
                Err(EvalErr::InvalidNilTerminator(operand_list))
            } else {
                self.push(self.allocator.nil())?;
                Ok(OP_COST)
            }
        }
    }

    fn eval_pair(&mut self, program: NodePtr, env: NodePtr) -> Result<Cost> {
        #[cfg(feature = "pre-eval")]
        if let Some(pre_eval) = &self.pre_eval {
            if let Some(post_eval) = pre_eval(self.allocator, program, env)? {
                self.posteval_stack.push(post_eval);
                self.op_stack.push(Operation::PostEval);
            }
        };

        // put a bunch of ops on op_stack
        let SExp::Pair(op_node, op_list) = self.allocator.sexp(program) else {
            // the program is just a bitfield path through the env tree
            let r = match self.allocator.node(program) {
                NodeVisitor::Buffer(buf) => traverse_path(self.allocator, buf, env)?,
                NodeVisitor::U32(val) => traverse_path_fast(self.allocator, val, env)?,
                NodeVisitor::Pair(_, _) => {
                    return Err(EvalErr::InvalidOpArg(
                        program,
                        "expected atom, got pair".to_string(),
                    ))?;
                }
            };
            self.push(r.1)?;
            return Ok(r.0);
        };

        match self.allocator.sexp(op_node) {
            SExp::Pair(new_operator, _) => {
                let [inner] = get_args::<1>(
                    self.allocator,
                    op_node,
                    "in the ((X)...) syntax, the inner list",
                )?;
                if let SExp::Pair(_, _) = self.allocator.sexp(inner) {
                    return Err(EvalErr::InvalidOpArg(
                        program,
                        "in ((X)...) syntax X must be lone atom".to_string(),
                    ));
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

    fn swap_eval_op(&mut self) -> Result<Cost> {
        let v2 = self.pop()?;
        let program: NodePtr = self.pop()?;
        let env: NodePtr = *self.env_stack.last().ok_or(EvalErr::InternalError(
            program,
            "environment stack empty".to_string(),
        ))?;
        self.push(v2)?;

        // on the way back, build a list from the values
        self.op_stack.push(Operation::Cons);
        self.account_op_push();

        self.eval_pair(program, env)
    }

    fn parse_softfork_arguments(&self, args: NodePtr) -> Result<(OperatorSet, NodePtr, NodePtr)> {
        let [_cost, extension, program, env] = get_args::<4>(self.allocator, args, "softfork")?;

        let extension =
            self.dialect
                .softfork_extension(uint_atom::<4>(self.allocator, extension, "softfork")? as u32);
        if extension == OperatorSet::Default {
            Err(EvalErr::UnknownSoftforkExtension)
        } else {
            Ok((extension, program, env))
        }
    }

    fn apply_op(&mut self, current_cost: Cost, max_cost: Cost) -> Result<Cost> {
        let operand_list = self.pop()?;
        let operator = self.pop()?;
        if self.env_stack.pop().is_none() {
            return Err(EvalErr::InternalError(
                operator,
                "environment stack empty".to_string(),
            ));
        }
        let op_atom = self.allocator.small_number(operator);

        if op_atom == Some(self.dialect.apply_kw()) {
            let [new_operator, env] = get_args::<2>(self.allocator, operand_list, "apply")?;
            self.eval_pair(new_operator, env).map(|c| c + APPLY_COST)
        } else if op_atom == Some(self.dialect.softfork_kw()) {
            let expected_cost = uint_atom::<8>(
                self.allocator,
                first(self.allocator, operand_list)?,
                "softfork",
            )?;
            if expected_cost > max_cost {
                return Err(EvalErr::CostExceeded);
            }
            if expected_cost == 0 {
                return Err(EvalErr::CostExceeded);
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
                        self.push(self.allocator.nil())?;
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

    fn exit_guard(&mut self, current_cost: Cost) -> Result<Cost> {
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
            return Err(EvalErr::SoftforkCostMismatch);
        }

        // restore the allocator to the state when we entered the softfork guard
        // This is an optimization to reclaim all heap space allocated by the
        // softfork program. Since the softfork always return nil, no value can
        // escape the softfork program, and it's therefore safe to restore the
        // heap
        self.allocator.restore_checkpoint(&guard.allocator_state);

        // the softfork always returns nil, pop the value pushed by the
        // evaluation of the program and push nil instead
        self.pop()
            .expect("internal error, softfork program did not push value onto stack");

        self.push(self.allocator.nil())?;

        Ok(0)
    }

    pub fn run_program(&mut self, program: NodePtr, env: NodePtr, max_cost: Cost) -> Response {
        self.val_stack = vec![];
        self.op_stack = vec![];

        // max_cost is always in effect, and necessary to prevent wrap-around of
        // the cost integer.
        let max_cost = if max_cost == 0 { Cost::MAX } else { max_cost };
        // We would previously allocate an atom to hold the max cost for the program.
        // Since we don't anymore we need to increment the ghost atom counter to remain
        // backwards compatible with the atom count limit
        self.allocator.add_ghost_atom(1)?;
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
                return Err(EvalErr::CostExceeded);
            }
            let top = self.op_stack.pop();
            let op = match top {
                Some(f) => f,
                None => break,
            };
            cost += match op {
                Operation::Apply => self.apply_op(cost, effective_max_cost - cost)?,
                Operation::ExitGuard => self.exit_guard(cost)?,
                Operation::Cons => self.cons_op()?,
                Operation::SwapEval => self.swap_eval_op()?,
                #[cfg(feature = "pre-eval")]
                Operation::PostEval => {
                    let f = self.posteval_stack.pop().unwrap();
                    let peek: Option<NodePtr> = self.val_stack.last().copied();
                    f(self.allocator, peek);
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
    rpc.counters.small_atom_count = rpc.allocator.small_atom_count() as u32;
    rpc.counters.pair_count = rpc.allocator.pair_count() as u32;
    rpc.counters.heap_size = rpc.allocator.heap_size() as u32;
    (rpc.counters, ret)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::chia_dialect::{ENABLE_KECCAK_OPS_OUTSIDE_GUARD, NO_UNKNOWN_OPS};
    use crate::test_ops::parse_exp;

    use rstest::rstest;

    struct RunProgramTest<'a> {
        prg: &'a str,
        args: &'a str,
        flags: u32,
        result: Option<&'a str>,
        cost: Cost,
        err: &'a str,
    }

    const TEST_CASES: &[RunProgramTest] = &[
        RunProgramTest {
            prg: "(/ (q . 10) (q . -3))",
            args: "()",
            flags: 0,
            result: Some("-4"),
            cost: 1047,
            err: "",
        },
        RunProgramTest {
            prg: "(/ (q . -10) (q . 3))",
            args: "()",
            flags: 0,
            result: Some("-4"),
            cost: 1047,
            err: "",
        },
        RunProgramTest {
            prg: "(/ (q . -1) (q . 2))",
            args: "()",
            flags: 0,
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
            err: "InvalidOperatorArg: apply takes exactly 2 argument(s)",
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
            err: "InvalidOperatorArg: apply takes exactly 2 argument(s)",
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
            err: "InvalidOperatorArg: in the ((X)...) syntax, the inner list takes exactly 1 argument(s)",
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
            err: "InvalidOperatorArg: softfork takes exactly 4 argument(s)",
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
            err: "InvalidOperatorArg: softfork takes exactly 4 argument(s)",
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
            err: "InvalidOperatorArg: softfork takes exactly 4 argument(s)",
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
            err: "InvalidOperatorArg: softfork requires positive int arg",
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
            err: "InvalidOperatorArg: softfork requires u32 arg (with no leading zeros)",
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
            err: "InvalidOperatorArg: Requires Int Argument: softfork",
        },

        // the cost value is checked in consensus mode as well
        RunProgramTest {
            prg: "(softfork (q . 1000))",
            args: "()",
            flags: 0,
            result: None,
            cost: 1000,
            err: "cost exceeded or below zero",
        },
        // the cost parameter is mandatory
        RunProgramTest {
            prg: "(softfork)",
            args: "()",
            flags: 0,
            result: None,
            cost: 0,
            err: "InvalidOperatorArg: first of non-cons",
        },
        RunProgramTest {
            prg: "(softfork (q . 0))",
            args: "()",
            flags: 0,
            result: None,
            cost: 1000,
            err: "cost exceeded or below zero",
        },
        // negative costs are not allowed
        RunProgramTest {
            prg: "(softfork (q . -1))",
            args: "()",
            flags: 0,
            result: None,
            cost: 1000,
            err: "InvalidOperatorArg: softfork requires positive int arg",
        },
        RunProgramTest {
            prg: "(softfork (q 1 2 3))",
            args: "()",
            flags: 0,
            result: None,
            cost: 1000,
            err: "InvalidOperatorArg: Requires Int Argument: softfork",
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
            err: "cost exceeded or below zero",
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

        // without the flag to enable the keccak extensions, it's an unknown extension
        RunProgramTest {
            prg: "(softfork (q . 161) (q . 2) (q . (q . 42)) (q . ()))",
            args: "()",
            flags: NO_UNKNOWN_OPS,
            result: None,
            cost: 10000,
            err: "unknown softfork extension",
        },

        // coinid is also available under softfork extension 1
        RunProgramTest {
            prg: "(softfork (q . 1432) (q . 1) (q a (i (= (coinid (q . 0x1234500000000000000000000000000000000000000000000000000000000000) (q . 0x6789abcdef000000000000000000000000000000000000000000000000000000) (q . 123456789)) (q . 0x69bfe81b052bfc6bd7f3fb9167fec61793175b897c16a35827f947d5cc98e4bc)) (q . 0) (q x)) (q . ())) (q . ()))",
            args: "()",
            flags: 0,
            result: Some("()"),
            cost: 1513,
            err: "",
        },

        // keccak256 is available when the softfork has activated
        RunProgramTest {
            prg: "(softfork (q . 1134) (q . 1) (q a (i (= (keccak256 (q . \"foobar\")) (q . 0x38d18acb67d25c8bb9942764b62f18e17054f66a817bd4295423adf9ed98873e)) (q . 0) (q x)) (q . ())) (q . ()))",
            args: "()",
            flags: 0,
            result: Some("()"),
            cost: 1215,
            err: "",
        },
        // make sure keccak is actually executed, by comparing with the wrong output
        RunProgramTest {
            prg: "(softfork (q . 1134) (q . 1) (q a (i (= (keccak256 (q . \"foobar\")) (q . 0x58d18acb67d25c8bb9942764b62f18e17054f66a817bd4295423adf9ed98873e)) (q . 0) (q x)) (q . ())) (q . ()))",
            args: "()",
            flags: 0,
            result: None,
            cost: 1215,
            err: "clvm raise",
        },

        // === HARD FORK ===
        // new operators *outside* the softfork guard

        // keccak256 is available outside the guard with the appropriate flag
        RunProgramTest {
            prg: "(a (i (= (keccak256 (q . \"foobar\")) (q . 0x38d18acb67d25c8bb9942764b62f18e17054f66a817bd4295423adf9ed98873e)) (q . 0) (q x)) (q . ()))",
            args: "()",
            flags: ENABLE_KECCAK_OPS_OUTSIDE_GUARD,
            result: Some("()"),
            cost: 994,
            err: "",
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
            flags: 0,
            result: Some("0x69bfe81b052bfc6bd7f3fb9167fec61793175b897c16a35827f947d5cc98e4bc"),
            cost: 861,
            err: "",
        },
        RunProgramTest {
            prg: "(coinid (q . 0x1234500000000000000000000000000000000000000000000000000000000000) (q . 0x6789abcdef000000000000000000000000000000000000000000000000000000) (q . 0x000123456789))",
            args: "()",
            flags: 0,
            result: None,
            cost: 861,
            err: "InvalidOperatorArg: CoinID Error: Invalid Amount: Amount has leading zeroes",
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
            err: "Secp256 Verify Error: failed",
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
            err: "Secp256 Verify Error: failed",
        },
    ];

    fn check(res: (NodePtr, &str)) -> NodePtr {
        assert_eq!(res.1, "");
        res.0
    }

    fn run_test_case(t: &RunProgramTest) {
        use crate::chia_dialect::ChiaDialect;
        use crate::test_ops::node_eq;
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
                assert_eq!(expected_cost_exceeded, EvalErr::CostExceeded);
            }
            Err(err) => {
                println!("FAILED: {err}");
                assert_eq!(err.to_string(), t.err);
                assert!(expected_result.is_none());
            }
        }
    }

    #[test]
    fn test_run_program() {
        for t in TEST_CASES {
            run_test_case(t);
        }
    }

    // the test cases for this test consists of:
    // prg: the program to run inside the softfork guard
    // cost: the expected cost of the program (the test adds the apply-operator)
    // enabled: the softfork extension number that enables operator in prg
    // hard_fork_flag: the flag that enables the program to be run outside the guard
    // err: the expected error message, empty string means OK
    // The test programs are carefully crafted such that they fail with "clvm raise"
    // when run in consensus mode and the operators are unknown. e.g. (coinid ...)
    // returns NIL in that case, which compares not equal to the coin ID, which
    // raises the exception.
    // This property is relied on for the non-mempool and fork-not-activated cases.
    #[rstest]
    // make sure we can execute the coinid operator under softfork 0
    // this program raises an exception if the computed coin ID matches the
    // expected
    #[case::coinid(
        "(i (= (coinid (q . 0x1234500000000000000000000000000000000000000000000000000000000000) (q . 0x6789abcdef000000000000000000000000000000000000000000000000000000) (q . 123456789)) (q . 0x69bfe81b052bfc6bd7f3fb9167fec61793175b897c16a35827f947d5cc98e4bd)) (q . 0) (q x))",
        (1432, 0, 0),
        "clvm raise")
    ]
    // also test the opposite. This program is the same as above but it raises
    // if the coin ID is a mismatch
    #[case::coinid(
        "(i (= (coinid (q . 0x1234500000000000000000000000000000000000000000000000000000000000) (q . 0x6789abcdef000000000000000000000000000000000000000000000000000000) (q . 123456789)) (q . 0x69bfe81b052bfc6bd7f3fb9167fec61793175b897c16a35827f947d5cc98e4bc)) (q . 0) (q x))",
        (1432, 0, 0),
        ""
    )]
    // modpow
    #[case::modpow(
        "(i (= (modpow (q . 12345) (q . 6789) (q . 44444444444)) (q . 13456191581)) (q . 0) (q x))",
        (18241, 0, 0),
        ""
    )]
    #[case::modpow(
        "(i (= (modpow (q . 12345) (q . 6789) (q . 44444444444)) (q . 13456191582)) (q . 0) (q x))",
        (18241, 0, 0),
        "clvm raise"
    )]
    // mod
    #[case::modulus(
        "(i (= (% (q . 80001) (q . 73)) (q . 66)) (q . 0) (q x))",
        (1564, 0, 0),
        ""
    )]
    #[case::modulus(
        "(i (= (% (q . 80001) (q . 73)) (q . 67)) (q . 0) (q x))",
        (1564, 0, 0),
        "clvm raise"
    )]
    // g1_multiply
    #[case::g1_mul(
        "(i (= (g1_multiply  (q . 0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb) (q . 2)) (q . 0xa572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4e)) (q . 0) (q x))",
        (706634, 0, 0),
        ""
    )]
    #[case::g1_mul(
        "(i (= (g1_multiply  (q . 0x97f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb) (q . 2)) (q . 0xa572cbea904d67468808c8eb50a9450c9721db309128012543902d0ac358a62ae28f75bb8f1c7c42c39a8c5529bf0f4f)) (q . 0) (q x))",
        (706634, 0, 0),
        "clvm raise"
    )]
    #[case::g1_neg(
        "(i (= (g1_negate (q . 0xb7f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb)) (q . 0xb7f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb)) (q . 0) (q x))",
        (706634, 0, 0),
        "clvm raise"
    )]
    #[case::g1_neg(
        "(i (= (g1_negate (q . 0xb2f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb)) (q . 0xb7f1d3a73197d7942695638c4fa9ac0fc3688c4f9774b905a14e3a3f171bac586c55e83ff97a1aeffb3af00adb22c6bb)) (q . 0) (q x))",
        (706634, 0, 0),
        "InvalidOperatorArg: atom is not a G1 point"
    )]
    #[case::g2_add(
        "(i (= (g2_add (q . 0x93e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8) (q . 0x93e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8)) (q . 0xaa4edef9c1ed7f729f520e47730a124fd70662a904ba1074728114d1031e1572c6c886f6b57ec72a6178288c47c335771638533957d540a9d2370f17cc7ed5863bc0b995b8825e0ee1ea1e1e4d00dbae81f14b0bf3611b78c952aacab827a053)) (q . 0) (q x))",
        (3981700, 0, 0),
        ""
    )]
    #[case::g2_add(
        "(i (= (g2_add (q . 0x93e12b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8) (q . 0x93e02b6052719f607dacd3a088274f65596bd0d09920b61ab5da61bbdc7f5049334cf11213945d57e5ac7d055d042b7e024aa2b2f08f0a91260805272dc51051c6e47ad4fa403b02b4510b647ae3d1770bac0326a805bbefd48056c8c121bdb8)) (q . 0xaa4edef9c1ed7f729f520e47730a124fd70662a904ba1074728114d1031e1572c6c886f6b57ec72a6178288c47c335771638533957d540a9d2370f17cc7ed5863bc0b995b8825e0ee1ea1e1e4d00dbae81f14b0bf3611b78c952aacab827a053)) (q . 0) (q x))",
        (3981700, 0, 0),
        "InvalidAllocatorArg: atom is not a G2 point"
    )]
    #[case::keccak(
        "(i (= (keccak256 (q . \"foobar\")) (q . 0x38d18acb67d25c8bb9942764b62f18e17054f66a817bd4295423adf9ed98873e)) (q . 0) (q x))",
        (1134, 1, ENABLE_KECCAK_OPS_OUTSIDE_GUARD),
        ""
    )]
    #[case::keccak(
        "(i (= (keccak256 (q . \"foobar\")) (q . 0x38d18acb67d25c8bb9942764b62f18e17054f66a817bd4295423adf9ed98873f)) (q . 0) (q x))",
        (1134, 1, ENABLE_KECCAK_OPS_OUTSIDE_GUARD),
        "clvm raise"
    )]
    fn test_softfork(
        #[case] prg: &'static str,
        #[case] fields: (u64, u8, u32), // cost, enabled, hard_fork_flag
        #[case] err: &'static str,
        #[values(0)] flags: u32,
        #[values(false, true)] mempool: bool,
        #[values(0, 1, 2)] test_ext: u8,
    ) {
        let (cost, enabled, hard_fork_flag) = fields;
        let softfork_prg =
            format!("(softfork (q . {cost}) (q . {test_ext}) (q . (a {prg} (q . 0))) (q . 0))");

        let flags = flags | if mempool { NO_UNKNOWN_OPS } else { 0 };

        // softfork extensions that are enabled
        #[allow(clippy::match_like_matches_macro)]
        let ext_enabled = match test_ext {
            0 => true, // BLS
            1 => true, // KECCAK
            _ => false,
        };

        println!("mempool: {mempool} ext: {test_ext} flags: {flags}");
        let expect_err = match (ext_enabled as u8, (test_ext >= enabled) as u8) {
            // the extension we're running has not been activated, and we're not
            // running an extension that supports the operator
            (0, 0) => {
                if mempool {
                    "unimplemented operator"
                } else {
                    ""
                }
            }
            // the softfork extension hasn't been activated yet. It's a failure in
            // mempool mode but ignored in consensus mode
            (0, 1) => {
                if mempool {
                    "unknown softfork extension"
                } else {
                    ""
                }
            }
            // the extension we're invoking has been enabled, but the operator is
            // not part of this extension. In mempool mode it's an error, in
            // consensus mode the operator is considered unknown, returning
            // NIL/false. This in turn will make the return value test fail, and
            // raise an exception.
            (1, 0) => {
                if mempool {
                    "unimplemented operator"
                } else {
                    "clvm raise"
                }
            }
            // the extension we're running has been activated, and we're running an
            // extension the operator is available in. The program is executed and
            // we get the expected result.
            (1, 1) => err,
            _ => unreachable!(),
        };

        println!("expect: {expect_err} cost: {cost}");
        let t = RunProgramTest {
            prg: softfork_prg.as_str(),
            args: "()",
            flags,
            result: if expect_err.is_empty() {
                Some("()")
            } else {
                None
            },
            cost: cost + 81,
            err: expect_err,
        };

        run_test_case(&t);

        // now test outside the guard (should fail unless hard_fork_flag is set).

        let outside_guard_prg = format!("(a {prg} (q . 0))");

        // without the hard fork flag
        println!("outside guard, no hard fork");
        let t = RunProgramTest {
            prg: outside_guard_prg.as_str(),
            args: "()",
            flags,
            result: if err.is_empty() && hard_fork_flag == 0 {
                Some("()")
            } else {
                None
            },
            cost: cost - 140,
            err: if hard_fork_flag == 0 {
                err
            } else if mempool {
                "unimplemented operator"
            } else {
                "clvm raise"
            },
        };
        run_test_case(&t);

        // with the hard fork flag
        println!("outside guard, hard fork activated");
        let t = RunProgramTest {
            prg: outside_guard_prg.as_str(),
            args: "()",
            flags: flags | hard_fork_flag,
            result: if err.is_empty() { Some("()") } else { None },
            cost: cost - 140,
            err,
        };
        run_test_case(&t);
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
        assert_eq!(counters.atom_count, 998);
        assert_eq!(counters.small_atom_count, 1042);
        assert_eq!(counters.pair_count, 22077);
        assert_eq!(counters.heap_size, 769963);

        assert_eq!(result.unwrap().0, cost);
    }
}
