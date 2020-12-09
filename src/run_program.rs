use crate::allocator::{Allocator, SExp};
use crate::node::{Node, U8};

use super::number::{node_from_number, number_from_u8, Number};

use super::types::{EvalErr, OperatorHandler, PreEval, Reduction};

const QUOTE_COST: u32 = 1;
const SHIFT_COST_PER_LIMB: u32 = 1;

type RPCOperator<T> = dyn FnOnce(&mut RunProgramContext<T>) -> Result<u32, EvalErr<T>>;

// `run_program` has two stacks: the operand stack (of `Node` objects) and the
// operator stack (of RPCOperators)

pub struct RunProgramContext<'a, T> {
    allocator: &'a dyn Allocator<T, U8>,
    quote_kw: u8,
    operator_lookup: &'a OperatorHandler<T, U8>,
    pre_eval: Option<PreEval<T>>,
    val_stack: Vec<T>,
    op_stack: Vec<Box<RPCOperator<T>>>,
}

impl<T> RunProgramContext<'_, T>
where
    T: Clone,
{
    pub fn pop(&mut self) -> Result<T, EvalErr<T>> {
        let v = self.val_stack.pop();
        match v {
            None => {
                let node: T = self.allocator.null();
                self.allocator
                    .err(&node, "runtime error: value stack empty")
            }
            Some(k) => Ok(k),
        }
    }
    pub fn push(&mut self, node: T) {
        self.val_stack.push(node);
    }
}

fn limbs_for_int(node_index: Number) -> u32 {
    let mut v = 1;
    let mut ni = node_index;
    let c: Number = 256.into();
    loop {
        if ni < c {
            break;
        };
        v += 1;
        ni >>= 8;
    }
    v
}

fn traverse_path<T>(
    allocator: &dyn Allocator<T, U8>,
    path_node: &T,
    args: &T,
) -> Result<Reduction<T>, EvalErr<T>>
where
    T: Clone,
{
    /*
    Follow integer `NodePath` down a tree.
    */
    let node_index: Option<Number> = match allocator.sexp(path_node) {
        SExp::Atom(atom) => number_from_u8(&atom),
        _ => None,
    };

    let mut node_index: Number = node_index.unwrap();
    let one: Number = (1).into();
    let mut cost = 1;
    let mut arg_list: T = args.clone();
    loop {
        if node_index <= one {
            break;
        }
        match allocator.sexp(&arg_list) {
            SExp::Atom(_) => {
                return Err(EvalErr(arg_list.clone(), "path into atom".into()));
            }
            SExp::Pair(left, right) => {
                arg_list = if node_index & one == one {
                    right.clone()
                } else {
                    left.clone()
                };
            }
        };
        cost += SHIFT_COST_PER_LIMB * limbs_for_int(node_index);
        node_index >>= 1;
    }
    Ok(Reduction(cost, arg_list))
}

fn swap_op<T>(rpc: &mut RunProgramContext<T>) -> Result<u32, EvalErr<T>>
where
    T: Clone,
{
    /* Swap the top two operands. */
    let v2 = rpc.pop()?;
    let v1 = rpc.pop()?;
    rpc.push(v2);
    rpc.push(v1);
    Ok(0)
}

fn cons_op<T>(rpc: &mut RunProgramContext<T>) -> Result<u32, EvalErr<T>>
where
    T: Clone,
{
    /* Join the top two operands. */
    let v1 = rpc.pop()?;
    let v2 = rpc.pop()?;
    rpc.push(rpc.allocator.from_pair(&v1, &v2));
    Ok(0)
}

fn eval_op_atom(
    rpc: &mut RunProgramContext<Node>,
    op_atom: &[u8],
    operator_node: &Node,
    operand_list: &Node,
    args: &Node,
) -> Result<u32, EvalErr<Node>> {
    // special case check for quote
    if op_atom.len() == 1 && op_atom[0] == rpc.quote_kw {
        match rpc.allocator.sexp(operand_list) {
            SExp::Atom(_) => rpc
                .allocator
                .err(operand_list, "quote requires exactly 1 parameter"),
            SExp::Pair(quoted_val, nil) => {
                if rpc.allocator.nullp(&nil) {
                    rpc.push(quoted_val);
                    Ok(QUOTE_COST)
                } else {
                    rpc.allocator
                        .err(operand_list, "quote requires exactly 1 parameter")
                }
            }
        }
    } else {
        rpc.op_stack.push(Box::new(apply_op));
        rpc.push(operator_node.clone());
        let mut operands = operand_list.clone();
        loop {
            if rpc.allocator.nullp(&operands) {
                break;
            }
            rpc.op_stack.push(Box::new(cons_op));
            rpc.op_stack.push(Box::new(eval_op));
            rpc.op_stack.push(Box::new(swap_op));
            match rpc.allocator.sexp(&operands) {
                SExp::Atom(_) => {
                    return Err(EvalErr(operand_list.clone(), "bad operand list".into()))
                }
                SExp::Pair(first, rest) => {
                    let new_pair = rpc.allocator.from_pair(&first, args);
                    rpc.push(new_pair);
                    operands = rest.clone();
                }
            }
        }
        rpc.push(rpc.allocator.null());
        Ok(1)
    }
}

fn eval_pair(
    rpc: &mut RunProgramContext<Node>,
    program: &Node,
    args: &Node,
) -> Result<u32, EvalErr<Node>> {
    // put a bunch of ops on op_stack
    match rpc.allocator.sexp(program) {
        // the program is just a bitfield path through the args tree
        SExp::Atom(_) => {
            let r: Reduction<Node> = traverse_path(rpc.allocator, &program, &args)?;
            rpc.push(r.1);
            Ok(r.0)
        }
        // the program is an operator and a list of operands
        SExp::Pair(operator_node, operand_list) => match rpc.allocator.sexp(&operator_node) {
            SExp::Pair(_, _) => {
                // the operator is also a list, so we need two evals here
                rpc.push(rpc.allocator.from_pair(&operator_node, &args));
                rpc.op_stack.push(Box::new(eval_op));
                rpc.op_stack.push(Box::new(eval_op));
                Ok(1)
            }
            SExp::Atom(op_atom) => eval_op_atom(rpc, &op_atom, &operator_node, &operand_list, args),
        },
    }
}

fn eval_op(rpc: &mut RunProgramContext<Node>) -> Result<u32, EvalErr<Node>> {
    /*
    Pop the top value and treat it as a (program, args) pair, and manipulate
    the op & value stack to evaluate all the arguments and apply the operator.
    */

    let pair: Node = rpc.pop()?;
    match rpc.allocator.sexp(&pair) {
        SExp::Atom(_) => rpc.allocator.err(&pair, "pair expected"),
        SExp::Pair(program, args) => {
            let post_eval = match rpc.pre_eval {
                None => None,
                Some(ref pre_eval) => pre_eval(&program, &args)?,
            };
            match post_eval {
                None => (),
                Some(post_eval) => {
                    let new_function = Box::new(move |rpc: &mut RunProgramContext<Node>| {
                        let peek: Option<&Node> = rpc.val_stack.last();
                        post_eval(peek);
                        Ok(0)
                    });
                    rpc.op_stack.push(new_function);
                }
            };
            eval_pair(rpc, &program, &args)
        }
    }
}

fn apply_op(rpc: &mut RunProgramContext<Node>) -> Result<u32, EvalErr<Node>> {
    let operand_list = rpc.pop()?;
    let operator = rpc.pop()?;
    match rpc.allocator.sexp(&operator) {
        SExp::Pair(_, _) => Err(EvalErr(operator, "internal error".into())),
        SExp::Atom(op_atom) => {
            let r = (rpc.operator_lookup)(rpc.allocator, &op_atom, &operand_list)?;
            rpc.push(r.1);
            Ok(r.0)
        }
    }
}

pub fn run_program(
    allocator: &dyn Allocator<Node, U8>,
    program: &Node,
    args: &Node,
    quote_kw: u8,
    max_cost: u32,
    operator_lookup: &OperatorHandler<Node, U8>,
    pre_eval: Option<PreEval<Node>>,
) -> Result<Reduction<Node>, EvalErr<Node>> {
    let values: Vec<Node> = vec![allocator.from_pair(program, args)];
    let op_stack: Vec<Box<RPCOperator<Node>>> = vec![Box::new(eval_op)];

    let mut rpc = RunProgramContext {
        allocator,
        quote_kw,
        operator_lookup,
        pre_eval,
        val_stack: values,
        op_stack,
    };

    let mut cost: u32 = 0;

    loop {
        let top = rpc.op_stack.pop();
        match top {
            Some(f) => {
                cost += f(&mut rpc)?;
            }
            None => break,
        }
        if cost > max_cost && max_cost > 0 {
            let max_cost: Number = max_cost.into();
            let n: Node = node_from_number(allocator, max_cost);
            return Err(EvalErr(n, "cost exceeded".into()));
        }
    }

    Ok(Reduction(cost, rpc.pop()?))
}
