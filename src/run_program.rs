use crate::allocator::{Allocator, AtomBuf, NodePtr, SExp};
use crate::cost::Cost;
use crate::err_utils::err;
use crate::node::Node;
use crate::reduction::{EvalErr, Reduction, Response};

use crate::number::{ptr_from_number, Number};

// lowered from 46
const QUOTE_COST: Cost = 20;
// lowered from 138
const APPLY_COST: Cost = 90;

// lowered from measured 147 per bit. It doesn't seem to take this long in
// practice
const TRAVERSE_BASE_COST: Cost = 40;
const TRAVERSE_COST_PER_ZERO_BYTE: Cost = 4;
const TRAVERSE_COST_PER_BIT: Cost = 4;

pub trait OperatorHandler {
    fn op(&self, allocator: &mut Allocator, op: NodePtr, args: NodePtr, max_cost: Cost)
        -> Response;
}

pub type PreEval =
    Box<dyn Fn(&mut Allocator, NodePtr, NodePtr) -> Result<Option<Box<PostEval>>, EvalErr>>;

pub type PostEval = dyn Fn(Option<NodePtr>);

#[repr(u8)]
enum Operation {
    Apply,
    Cons,
    Eval,
    Swap,
    PostEval,
}

// `run_program` has two stacks: the operand stack (of `Node` objects) and the
// operator stack (of RpcOperators)

pub struct RunProgramContext<'a> {
    allocator: &'a mut Allocator,
    quote_kw: u8,
    apply_kw: u8,
    operator_lookup: Box<dyn OperatorHandler>,
    pre_eval: Option<PreEval>,
    posteval_stack: Vec<Box<PostEval>>,
    val_stack: Vec<NodePtr>,
    op_stack: Vec<Operation>,
}

impl<'a, 'h> RunProgramContext<'a> {
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
    pub fn push(&mut self, node: NodePtr) {
        self.val_stack.push(node);
    }
}

// return a bitmask with a single bit set, for the most significant set bit in
// the input byte
fn msb_mask(byte: u8) -> u8 {
    let mut byte = (byte | (byte >> 1)) as u32;
    byte |= byte >> 2;
    byte |= byte >> 4;
    debug_assert!((byte + 1) >> 1 <= 0x80);
    ((byte + 1) >> 1) as u8
}

// return the index of the first non-zero byte in buf. If all bytes are 0, the
// length (one past end) will be returned.
const fn first_non_zero(buf: &[u8]) -> usize {
    let mut c: usize = 0;
    while c < buf.len() && buf[c] == 0 {
        c += 1;
    }
    c
}

fn traverse_path(allocator: &Allocator, node_index: &[u8], args: NodePtr) -> Response {
    let mut arg_list: NodePtr = args;

    // find first non-zero byte
    let first_bit_byte_index = first_non_zero(node_index);

    let mut cost: Cost = TRAVERSE_BASE_COST
        + (first_bit_byte_index as Cost) * TRAVERSE_COST_PER_ZERO_BYTE
        + TRAVERSE_COST_PER_BIT;

    if first_bit_byte_index >= node_index.len() {
        return Ok(Reduction(cost, allocator.null()));
    }

    // find first non-zero bit (the most significant bit is a sentinel)
    let last_bitmask = msb_mask(node_index[first_bit_byte_index]);

    // follow through the bits, moving left and right
    let mut byte_idx = node_index.len() - 1;
    let mut bitmask = 0x01;
    while byte_idx > first_bit_byte_index || bitmask < last_bitmask {
        let is_bit_set: bool = (node_index[byte_idx] & bitmask) != 0;
        match allocator.sexp(arg_list) {
            SExp::Atom(_) => {
                return Err(EvalErr(arg_list, "path into atom".into()));
            }
            SExp::Pair(left, right) => {
                arg_list = *(if is_bit_set { &right } else { &left });
            }
        }
        if bitmask == 0x80 {
            bitmask = 0x01;
            byte_idx -= 1;
        } else {
            bitmask <<= 1;
        }
        cost += TRAVERSE_COST_PER_BIT;
    }
    Ok(Reduction(cost, arg_list))
}

fn augment_cost_errors(r: Result<Cost, EvalErr>, max_cost: NodePtr) -> Result<Cost, EvalErr> {
    if r.is_ok() {
        return r;
    }
    let e = r.unwrap_err();
    if &e.1 != "cost exceeded" {
        Err(e)
    } else {
        Err(EvalErr(max_cost, e.1))
    }
}

impl<'a> RunProgramContext<'a> {
    fn new(
        allocator: &'a mut Allocator,
        quote_kw: u8,
        apply_kw: u8,
        operator_lookup: Box<dyn OperatorHandler>,
        pre_eval: Option<PreEval>,
    ) -> Self {
        RunProgramContext {
            allocator,
            quote_kw,
            apply_kw,
            operator_lookup,
            pre_eval,
            posteval_stack: Vec::new(),
            val_stack: Vec::new(),
            op_stack: Vec::new(),
        }
    }

    fn swap_op(&mut self) -> Result<Cost, EvalErr> {
        /* Swap the top two operands. */
        let v2 = self.pop()?;
        let v1 = self.pop()?;
        self.push(v2);
        self.push(v1);
        Ok(0)
    }

    fn cons_op(&mut self) -> Result<Cost, EvalErr> {
        /* Join the top two operands. */
        let v1 = self.pop()?;
        let v2 = self.pop()?;
        let p = self.allocator.new_pair(v1, v2)?;
        self.push(p);
        Ok(0)
    }
}

impl<'a> RunProgramContext<'a>
where
    NodePtr: 'static,
{
    fn eval_op_atom(
        &mut self,
        op_buf: &AtomBuf,
        operator_node: NodePtr,
        operand_list: NodePtr,
        args: NodePtr,
    ) -> Result<Cost, EvalErr> {
        let op_atom = self.allocator.buf(op_buf);
        // special case check for quote
        if op_atom.len() == 1 && op_atom[0] == self.quote_kw {
            self.push(operand_list);
            Ok(QUOTE_COST)
        } else {
            self.op_stack.push(Operation::Apply);
            self.push(operator_node);
            let mut operands: NodePtr = operand_list;
            loop {
                if Node::new(self.allocator, operands).nullp() {
                    break;
                }
                self.op_stack.push(Operation::Cons);
                self.op_stack.push(Operation::Eval);
                self.op_stack.push(Operation::Swap);
                match self.allocator.sexp(operands) {
                    SExp::Atom(_) => return err(operand_list, "bad operand list"),
                    SExp::Pair(first, rest) => {
                        let new_pair = self.allocator.new_pair(first, args)?;
                        self.push(new_pair);
                        operands = rest;
                    }
                }
            }
            self.push(self.allocator.null());
            Ok(1)
        }
    }

    fn eval_pair(&mut self, program: NodePtr, args: NodePtr) -> Result<Cost, EvalErr> {
        // put a bunch of ops on op_stack
        let (op_node, op_list) = match self.allocator.sexp(program) {
            // the program is just a bitfield path through the args tree
            SExp::Atom(path) => {
                let r: Reduction = traverse_path(self.allocator, self.allocator.buf(&path), args)?;
                self.push(r.1);
                return Ok(r.0);
            }
            // the program is an operator and a list of operands
            SExp::Pair(operator_node, operand_list) => (operator_node, operand_list),
        };

        let op_atom = match self.allocator.sexp(op_node) {
            SExp::Pair(new_operator, must_be_nil) => {
                if let SExp::Atom(_) = self.allocator.sexp(new_operator) {
                    if Node::new(self.allocator, must_be_nil).nullp() {
                        self.push(new_operator);
                        self.push(op_list);
                        self.op_stack.push(Operation::Apply);
                        return Ok(APPLY_COST);
                    }
                }
                return Node::new(self.allocator, program)
                    .err("in ((X)...) syntax X must be lone atom");
            }
            SExp::Atom(op_atom) => op_atom,
        };

        self.eval_op_atom(&op_atom, op_node, op_list, args)
    }

    fn eval_op(&mut self) -> Result<Cost, EvalErr> {
        /*
        Pop the top value and treat it as a (program, args) pair, and manipulate
        the op & value stack to evaluate all the arguments and apply the operator.
        */

        let pair: NodePtr = self.pop()?;
        match self.allocator.sexp(pair) {
            SExp::Atom(_) => err(pair, "pair expected"),
            SExp::Pair(program, args) => {
                let post_eval = match self.pre_eval {
                    None => None,
                    Some(ref pre_eval) => pre_eval(&mut self.allocator, program, args)?,
                };
                if let Some(post_eval) = post_eval {
                    self.posteval_stack.push(post_eval);
                    self.op_stack.push(Operation::PostEval);
                };

                self.eval_pair(program, args)
            }
        }
    }

    fn apply_op(&mut self, max_cost: Cost) -> Result<Cost, EvalErr> {
        let operand_list = self.pop()?;
        let operator = self.pop()?;
        if let SExp::Pair(_, _) = self.allocator.sexp(operator) {
            return err(operator, "internal error");
        }
        let op_atom = self.allocator.atom(operator);
        if op_atom.len() == 1 && op_atom[0] == self.apply_kw {
            let operand_list = Node::new(self.allocator, operand_list);
            if operand_list.arg_count_is(2) {
                let new_operator = operand_list.first()?;
                let new_program = new_operator.node;
                let new_args = operand_list.rest()?.first()?.node;
                let new_pair = self.allocator.new_pair(new_program, new_args)?;
                self.push(new_pair);
                self.op_stack.push(Operation::Eval);
                Ok(APPLY_COST)
            } else {
                operand_list.err("apply requires exactly 2 parameters")
            }
        } else {
            let r = self
                .operator_lookup
                .op(self.allocator, operator, operand_list, max_cost)?;
            self.push(r.1);
            Ok(r.0)
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run_program(&mut self, program: NodePtr, args: NodePtr, max_cost: Cost) -> Response {
        self.val_stack = vec![self.allocator.new_pair(program, args)?];
        self.op_stack = vec![Operation::Eval];

        // max_cost is always in effect, and necessary to prevent wrap-around of
        // the cost integer.
        let max_cost = if max_cost == 0 { Cost::MAX } else { max_cost };

        let max_cost_number: Number = max_cost.into();
        let max_cost_ptr = ptr_from_number(self.allocator, &max_cost_number)?;

        let mut cost: Cost = 0;

        loop {
            let top = self.op_stack.pop();
            let op = match top {
                Some(f) => f,
                None => break,
            };
            cost += match op {
                Operation::Apply => {
                    augment_cost_errors(self.apply_op(max_cost - cost), max_cost_ptr)?
                }
                Operation::Cons => self.cons_op()?,
                Operation::Eval => augment_cost_errors(self.eval_op(), max_cost_ptr)?,
                Operation::Swap => self.swap_op()?,
                Operation::PostEval => {
                    let f = self.posteval_stack.pop().unwrap();
                    let peek: Option<NodePtr> = self.val_stack.last().copied();
                    f(peek);
                    0
                }
            };
            if cost > max_cost {
                return err(max_cost_ptr, "cost exceeded");
            }
        }
        Ok(Reduction(cost, self.pop()?))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_program(
    allocator: &mut Allocator,
    program: NodePtr,
    args: NodePtr,
    quote_kw: u8,
    apply_kw: u8,
    max_cost: Cost,
    operator_lookup: Box<dyn OperatorHandler>,
    pre_eval: Option<PreEval>,
) -> Response
where
    NodePtr: 'static,
{
    let mut rpc = RunProgramContext::new(allocator, quote_kw, apply_kw, operator_lookup, pre_eval);
    rpc.run_program(program, args, max_cost)
}

#[test]
fn test_msb_mask() {
    assert_eq!(msb_mask(0x0), 0x0);
    assert_eq!(msb_mask(0x01), 0x01);
    assert_eq!(msb_mask(0x02), 0x02);
    assert_eq!(msb_mask(0x04), 0x04);
    assert_eq!(msb_mask(0x08), 0x08);
    assert_eq!(msb_mask(0x10), 0x10);
    assert_eq!(msb_mask(0x20), 0x20);
    assert_eq!(msb_mask(0x40), 0x40);
    assert_eq!(msb_mask(0x80), 0x80);

    assert_eq!(msb_mask(0x44), 0x40);
    assert_eq!(msb_mask(0x2a), 0x20);
    assert_eq!(msb_mask(0xff), 0x80);
    assert_eq!(msb_mask(0x0f), 0x08);
}

#[test]
fn test_first_non_zero() {
    assert_eq!(first_non_zero(&[]), 0);
    assert_eq!(first_non_zero(&[1]), 0);
    assert_eq!(first_non_zero(&[0]), 1);
    assert_eq!(first_non_zero(&[0, 0, 0, 1, 1, 1]), 3);
    assert_eq!(first_non_zero(&[0, 0, 0, 0, 0, 0]), 6);
    assert_eq!(first_non_zero(&[1, 0, 0, 0, 0, 0]), 0);
}

#[test]
fn test_traverse_path() {
    use crate::allocator::Allocator;

    let mut a = Allocator::new();
    let nul = a.null();
    let n1 = a.new_atom(&[0, 1, 2]).unwrap();
    let n2 = a.new_atom(&[4, 5, 6]).unwrap();

    assert_eq!(traverse_path(&a, &[0], n1).unwrap(), Reduction(48, nul));
    assert_eq!(traverse_path(&a, &[0b1], n1).unwrap(), Reduction(44, n1));
    assert_eq!(traverse_path(&a, &[0b1], n2).unwrap(), Reduction(44, n2));

    // cost for leading zeros
    assert_eq!(
        traverse_path(&a, &[0, 0, 0, 0], n1).unwrap(),
        Reduction(60, nul)
    );

    let n3 = a.new_pair(n1, n2).unwrap();
    assert_eq!(traverse_path(&a, &[0b1], n3).unwrap(), Reduction(44, n3));
    assert_eq!(traverse_path(&a, &[0b10], n3).unwrap(), Reduction(48, n1));
    assert_eq!(traverse_path(&a, &[0b11], n3).unwrap(), Reduction(48, n2));
    assert_eq!(traverse_path(&a, &[0b11], n3).unwrap(), Reduction(48, n2));

    let list = a.new_pair(n1, nul).unwrap();
    let list = a.new_pair(n2, list).unwrap();

    assert_eq!(traverse_path(&a, &[0b10], list).unwrap(), Reduction(48, n2));
    assert_eq!(
        traverse_path(&a, &[0b101], list).unwrap(),
        Reduction(52, n1)
    );
    assert_eq!(
        traverse_path(&a, &[0b111], list).unwrap(),
        Reduction(52, nul)
    );

    // errors
    assert_eq!(
        traverse_path(&a, &[0b1011], list).unwrap_err(),
        EvalErr(nul, "path into atom".to_string())
    );
    assert_eq!(
        traverse_path(&a, &[0b1101], list).unwrap_err(),
        EvalErr(n1, "path into atom".to_string())
    );
    assert_eq!(
        traverse_path(&a, &[0b1001], list).unwrap_err(),
        EvalErr(n1, "path into atom".to_string())
    );
    assert_eq!(
        traverse_path(&a, &[0b1010], list).unwrap_err(),
        EvalErr(n2, "path into atom".to_string())
    );
    assert_eq!(
        traverse_path(&a, &[0b1110], list).unwrap_err(),
        EvalErr(n2, "path into atom".to_string())
    );
}
