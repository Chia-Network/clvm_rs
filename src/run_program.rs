use crate::allocator::{Allocator, SExp};
use crate::node::Node;
use crate::reduction::{EvalErr, Reduction, Response};

use crate::number::{node_from_number, Number};

use crate::types::OperatorHandler;

const QUOTE_COST: u32 = 1;
const TRAVERSE_COST_PER_ZERO_BYTE: u32 = 1;
const TRAVERSE_COST_PER_BIT: u32 = 1;
const APPLY_COST: u32 = 1;

pub type PreEval<T> = Box<dyn Fn(&T, &T) -> Result<Option<Box<PostEval<T>>>, EvalErr<T>>>;

pub type PostEval<T> = dyn Fn(Option<&T>);

type RPCOperator<T> = dyn FnOnce(&mut RunProgramContext<T>) -> Result<u32, EvalErr<T>>;

// `run_program` has two stacks: the operand stack (of `Node` objects) and the
// operator stack (of RPCOperators)

pub struct RunProgramContext<'a, T> {
    allocator: &'a dyn Allocator<T>,
    quote_kw: u8,
    apply_kw: u8,
    operator_lookup: &'a OperatorHandler<T>,
    pre_eval: Option<PreEval<T>>,
    val_stack: Vec<T>,
    op_stack: Vec<Box<RPCOperator<T>>>,
}

impl<T> RunProgramContext<'_, T> {
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

fn traverse_path<T>(allocator: &dyn Allocator<T>, path_node: &T, args: &T) -> Response<T> {
    /*
    Follow integer `NodePath` down a tree.
    */
    let node_index: &[u8] = match allocator.sexp(path_node) {
        SExp::Atom(a) => a,
        _ => panic!("problem in traverse_path"),
    };

    let mut arg_list: T = allocator.make_clone(args);

    // find first non-zero byte
    let mut first_bit_byte_index = 0;
    loop {
        if first_bit_byte_index >= node_index.len() || node_index[first_bit_byte_index] != 0 {
            break;
        }
        first_bit_byte_index += 1;
    }

    let mut cost: u32 = (1 + first_bit_byte_index as u32) * TRAVERSE_COST_PER_ZERO_BYTE;

    if first_bit_byte_index >= node_index.len() {
        arg_list = allocator.null();
    } else {
        // find first non-zero bit (the most significant bit is a sentinal)
        let mut last_bit_mask = 0x80;
        loop {
            if node_index[first_bit_byte_index] & last_bit_mask > 0 {
                break;
            }
            last_bit_mask >>= 1;
        }

        // follow through the bits, moving left and right
        let mut byte_idx = node_index.len() - 1;
        let mut bit_idx = 1;
        loop {
            if bit_idx > 128 {
                bit_idx = 1;
                byte_idx -= 1;
            }
            if byte_idx == first_bit_byte_index && bit_idx == last_bit_mask {
                break;
            }
            let is_bit_set: bool = node_index[byte_idx] & bit_idx == bit_idx;
            match allocator.sexp(&arg_list) {
                SExp::Atom(_) => {
                    return Err(EvalErr(arg_list, "path into atom".into()));
                }
                SExp::Pair(left, right) => {
                    arg_list = allocator.make_clone(if is_bit_set { &right } else { &left });
                }
            }
            bit_idx <<= 1;
            cost += TRAVERSE_COST_PER_BIT;
        }
    }
    Ok(Reduction(cost, arg_list))
}

fn swap_op<T>(rpc: &mut RunProgramContext<T>) -> Result<u32, EvalErr<T>> {
    /* Swap the top two operands. */
    let v2 = rpc.pop()?;
    let v1 = rpc.pop()?;
    rpc.push(v2);
    rpc.push(v1);
    Ok(0)
}

fn cons_op<T>(rpc: &mut RunProgramContext<T>) -> Result<u32, EvalErr<T>> {
    /* Join the top two operands. */
    let v1 = rpc.pop()?;
    let v2 = rpc.pop()?;
    rpc.push(rpc.allocator.from_pair(&v1, &v2));
    Ok(0)
}

fn eval_op_atom<T: 'static>(
    rpc: &mut RunProgramContext<T>,
    op_atom: &[u8],
    operator_node: &T,
    operand_list: &T,
    args: &T,
) -> Result<u32, EvalErr<T>> {
    // special case check for quote
    if op_atom.len() == 1 && op_atom[0] == rpc.quote_kw {
        match rpc.allocator.sexp(operand_list) {
            SExp::Atom(_) => rpc
                .allocator
                .err(&operand_list, "quote requires exactly 1 parameter"),
            SExp::Pair(quoted_val, nil) => {
                if rpc.allocator.nullp(&nil) {
                    rpc.push(quoted_val);
                    Ok(QUOTE_COST)
                } else {
                    rpc.allocator
                        .err(&operand_list, "quote requires exactly 1 parameter")
                }
            }
        }
    } else {
        rpc.op_stack.push(Box::new(apply_op));
        rpc.push(rpc.allocator.make_clone(operator_node));
        let mut operands = rpc.allocator.make_clone(operand_list);
        loop {
            if rpc.allocator.nullp(&operands) {
                break;
            }
            rpc.op_stack.push(Box::new(cons_op));
            rpc.op_stack.push(Box::new(eval_op));
            rpc.op_stack.push(Box::new(swap_op));
            match rpc.allocator.sexp(&operands) {
                SExp::Atom(_) => return rpc.allocator.err(operand_list, "bad operand list"),
                SExp::Pair(first, rest) => {
                    let new_pair = rpc.allocator.from_pair(&first, args);
                    rpc.push(new_pair);
                    operands = rpc.allocator.make_clone(&rest);
                }
            }
        }
        rpc.push(rpc.allocator.null());
        Ok(1)
    }
}

fn eval_pair<T: 'static>(
    rpc: &mut RunProgramContext<T>,
    program: &T,
    args: &T,
) -> Result<u32, EvalErr<T>> {
    // put a bunch of ops on op_stack
    match rpc.allocator.sexp(program) {
        // the program is just a bitfield path through the args tree
        SExp::Atom(_) => {
            let r: Reduction<T> = traverse_path(rpc.allocator, &program, &args)?;
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

fn eval_op<T: 'static>(rpc: &mut RunProgramContext<T>) -> Result<u32, EvalErr<T>> {
    /*
    Pop the top value and treat it as a (program, args) pair, and manipulate
    the op & value stack to evaluate all the arguments and apply the operator.
    */

    let pair: T = rpc.pop()?;
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
                    let new_function = Box::new(move |rpc: &mut RunProgramContext<T>| {
                        let peek: Option<&T> = rpc.val_stack.last();
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

fn apply_op<T: 'static>(rpc: &mut RunProgramContext<T>) -> Result<u32, EvalErr<T>> {
    let operand_list = rpc.pop()?;
    let operator = rpc.pop()?;
    match rpc.allocator.sexp(&operator) {
        SExp::Pair(_, _) => Err(EvalErr(operator, "internal error".into())),
        SExp::Atom(op_atom) => {
            if op_atom.len() == 1 && op_atom[0] == rpc.apply_kw {
                let operand_list = Node::new(rpc.allocator, operand_list);
                if operand_list.arg_count_is(2) {
                    let new_operator = operand_list.first()?;
                    let new_operand_list = operand_list.rest()?.first()?;
                    match new_operator.sexp() {
                        SExp::Pair(_, _) => {
                            let new_pair = rpc
                                .allocator
                                .from_pair(&new_operator.node, &new_operand_list.node);
                            rpc.push(new_pair);
                            rpc.op_stack.push(Box::new(eval_op));
                        }
                        SExp::Atom(_) => {
                            rpc.push(new_operator.node);
                            rpc.push(new_operand_list.node);
                            rpc.op_stack.push(Box::new(apply_op));
                        }
                    };
                    Ok(APPLY_COST)
                } else {
                    operand_list.err("apply requires exactly 2 parameters")
                }
            } else {
                let r = (rpc.operator_lookup)(rpc.allocator, &op_atom, &operand_list)?;
                rpc.push(r.1);
                Ok(r.0)
            }
        }
    }
}

pub fn run_program<T: 'static>(
    allocator: &dyn Allocator<T>,
    program: &T,
    args: &T,
    quote_kw: u8,
    apply_kw: u8,
    max_cost: u32,
    operator_lookup: &OperatorHandler<T>,
    pre_eval: Option<PreEval<T>>,
) -> Response<T> {
    let values: Vec<T> = vec![allocator.from_pair(program, args)];
    let op_stack: Vec<Box<RPCOperator<T>>> = vec![Box::new(eval_op)];

    let mut rpc = RunProgramContext {
        allocator,
        quote_kw,
        apply_kw,
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
            let n: Node<T> = node_from_number(allocator, &max_cost);
            return n.err("cost exceeded");
        }
    }

    Ok(Reduction(cost, rpc.pop()?))
}
