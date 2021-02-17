use crate::allocator::{Allocator, SExp};
use crate::cost::Cost;
use crate::err_utils::err;
use crate::node::Node;
use crate::reduction::{EvalErr, Reduction, Response};

use crate::number::{ptr_from_number, Number};

const QUOTE_COST: Cost = 1;
const TRAVERSE_COST_PER_ZERO_BYTE: Cost = 1;
const TRAVERSE_COST_PER_BIT: Cost = 1;
const APPLY_COST: Cost = 1;

pub trait OperatorHandler<T: Allocator> {
    fn op(
        &self,
        allocator: &mut T,
        op: <T as Allocator>::AtomBuf,
        args: &<T as Allocator>::Ptr,
    ) -> Response<<T as Allocator>::Ptr>;
}

pub type PreEval<A> = Box<
    dyn Fn(
        &mut A,
        &<A as Allocator>::Ptr,
        &<A as Allocator>::Ptr,
    ) -> Result<Option<Box<PostEval<A>>>, EvalErr<<A as Allocator>::Ptr>>,
>;

pub type PostEval<T> = dyn Fn(Option<&<T as Allocator>::Ptr>);

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

pub struct RunProgramContext<'a, T: Allocator> {
    allocator: &'a mut T,
    quote_kw: u8,
    apply_kw: u8,
    operator_lookup: Box<dyn OperatorHandler<T>>,
    pre_eval: Option<PreEval<T>>,
    posteval_stack: Vec<Box<PostEval<T>>>,
    val_stack: Vec<T::Ptr>,
    op_stack: Vec<Operation>,
}

impl<'a, 'h, T: Allocator> RunProgramContext<'a, T> {
    pub fn pop(&mut self) -> Result<T::Ptr, EvalErr<T::Ptr>> {
        let v: Option<T::Ptr> = self.val_stack.pop();
        match v {
            None => {
                let node: T::Ptr = self.allocator.null();
                err(node, "runtime error: value stack empty")
            }
            Some(k) => Ok(k),
        }
    }
    pub fn push(&mut self, node: T::Ptr) {
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
fn first_non_zero(buf: &[u8]) -> usize {
    let mut c: usize = 0;
    while c < buf.len() && buf[c] == 0 {
        c += 1;
    }
    c
}

fn traverse_path<T: Allocator>(
    allocator: &T,
    node_index: &[u8],
    args: &T::Ptr,
) -> Response<T::Ptr> {
    let mut arg_list: T::Ptr = args.clone();

    // find first non-zero byte
    let first_bit_byte_index = first_non_zero(node_index);

    let mut cost: Cost =
        (first_bit_byte_index as Cost) * TRAVERSE_COST_PER_ZERO_BYTE + TRAVERSE_COST_PER_BIT;

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
        match allocator.sexp(&arg_list) {
            SExp::Atom(_) => {
                return Err(EvalErr(arg_list, "path into atom".into()));
            }
            SExp::Pair(left, right) => {
                arg_list = (if is_bit_set { &right } else { &left }).clone();
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

impl<'a, 'h, T: Allocator> RunProgramContext<'a, T> {
    fn new(
        allocator: &'a mut T,
        quote_kw: u8,
        apply_kw: u8,
        operator_lookup: Box<dyn OperatorHandler<T>>,
        pre_eval: Option<PreEval<T>>,
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

    fn swap_op(&mut self) -> Result<Cost, EvalErr<T::Ptr>> {
        /* Swap the top two operands. */
        let v2 = self.pop()?;
        let v1 = self.pop()?;
        self.push(v2);
        self.push(v1);
        Ok(0)
    }

    fn cons_op(&mut self) -> Result<Cost, EvalErr<T::Ptr>> {
        /* Join the top two operands. */
        let v1 = self.pop()?;
        let v2 = self.pop()?;
        let p = self.allocator.new_pair(v1, v2)?;
        self.push(p);
        Ok(0)
    }
}

impl<'a, T: Allocator> RunProgramContext<'a, T>
where
    <T as Allocator>::Ptr: 'static,
{
    fn eval_op_atom(
        &mut self,
        op_buf: T::AtomBuf,
        operator_node: &T::Ptr,
        operand_list: &T::Ptr,
        args: &T::Ptr,
    ) -> Result<Cost, EvalErr<T::Ptr>> {
        let op_atom = self.allocator.buf(&op_buf);
        // special case check for quote
        if op_atom.len() == 1 && op_atom[0] == self.quote_kw {
            self.push(operand_list.clone());
            Ok(QUOTE_COST)
        } else {
            self.op_stack.push(Operation::Apply);
            self.push(operator_node.clone());
            let mut operands: T::Ptr = operand_list.clone();
            loop {
                if Node::new(self.allocator, operands.clone()).nullp() {
                    break;
                }
                self.op_stack.push(Operation::Cons);
                self.op_stack.push(Operation::Eval);
                self.op_stack.push(Operation::Swap);
                match self.allocator.sexp(&operands) {
                    SExp::Atom(_) => return err(operand_list.clone(), "bad operand list"),
                    SExp::Pair(first, rest) => {
                        let new_pair = self.allocator.new_pair(first, args.clone())?;
                        self.push(new_pair);
                        operands = rest.clone();
                    }
                }
            }
            self.push(self.allocator.null());
            Ok(1)
        }
    }

    fn eval_pair(&mut self, program: &T::Ptr, args: &T::Ptr) -> Result<Cost, EvalErr<T::Ptr>> {
        // put a bunch of ops on op_stack
        let (op_node, op_list) = match self.allocator.sexp(program) {
            // the program is just a bitfield path through the args tree
            SExp::Atom(path) => {
                let r: Reduction<T::Ptr> =
                    traverse_path(self.allocator, self.allocator.buf(&path), &args)?;
                self.push(r.1);
                return Ok(r.0);
            }
            // the program is an operator and a list of operands
            SExp::Pair(operator_node, operand_list) => (operator_node, operand_list),
        };

        let op_atom = match self.allocator.sexp(&op_node) {
            SExp::Pair(_, _) => {
                // the operator is also a list, so we need two evals here
                let p = self.allocator.new_pair(op_node, args.clone())?;
                self.push(p);
                self.op_stack.push(Operation::Eval);
                self.op_stack.push(Operation::Eval);
                return Ok(1);
            }
            SExp::Atom(op_atom) => op_atom,
        };

        self.eval_op_atom(op_atom, &op_node, &op_list, args)
    }

    fn eval_op(&mut self) -> Result<Cost, EvalErr<T::Ptr>> {
        /*
        Pop the top value and treat it as a (program, args) pair, and manipulate
        the op & value stack to evaluate all the arguments and apply the operator.
        */

        let pair: T::Ptr = self.pop()?;
        match self.allocator.sexp(&pair) {
            SExp::Atom(_) => err(pair, "pair expected"),
            SExp::Pair(program, args) => {
                let post_eval = match self.pre_eval {
                    None => None,
                    Some(ref pre_eval) => pre_eval(&mut self.allocator, &program, &args)?,
                };
                if let Some(post_eval) = post_eval {
                    self.posteval_stack.push(post_eval);
                    self.op_stack.push(Operation::PostEval);
                };

                self.eval_pair(&program, &args)
            }
        }
    }

    fn apply_op(&mut self) -> Result<Cost, EvalErr<T::Ptr>> {
        let operand_list = self.pop()?;
        let operator = self.pop()?;
        let opa = match self.allocator.sexp(&operator) {
            SExp::Pair(_, _) => {
                return Err(EvalErr(operator, "internal error".into()));
            }
            SExp::Atom(opa) => opa,
        };
        let op_atom = self.allocator.buf(&opa);
        if op_atom.len() == 1 && op_atom[0] == self.apply_kw {
            let operand_list = Node::new(self.allocator, operand_list);
            if operand_list.arg_count_is(2) {
                let new_operator = operand_list.first()?;
                let new_op_node = new_operator.node.clone();
                let new_op_list = operand_list.rest()?.first()?.node;
                match new_operator.sexp() {
                    SExp::Pair(_, _) => {
                        let new_pair = self.allocator.new_pair(new_op_node, new_op_list)?;
                        self.push(new_pair);
                        self.op_stack.push(Operation::Eval);
                    }
                    SExp::Atom(_) => {
                        self.push(new_op_node);
                        self.push(new_op_list);
                        self.op_stack.push(Operation::Apply);
                    }
                };
                Ok(APPLY_COST)
            } else {
                operand_list.err("apply requires exactly 2 parameters")
            }
        } else {
            let r = self
                .operator_lookup
                .op(self.allocator, opa, &operand_list)?;
            self.push(r.1);
            Ok(r.0)
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run_program(
        &mut self,
        program: &T::Ptr,
        args: &T::Ptr,
        max_cost: Cost,
    ) -> Response<T::Ptr> {
        self.val_stack = vec![self.allocator.new_pair(program.clone(), args.clone())?];
        self.op_stack = vec![Operation::Eval];

        let mut cost: Cost = 0;

        loop {
            let top = self.op_stack.pop();
            let op = match top {
                Some(f) => f,
                None => break,
            };
            cost += match op {
                Operation::Apply => self.apply_op()?,
                Operation::Cons => self.cons_op()?,
                Operation::Eval => self.eval_op()?,
                Operation::Swap => self.swap_op()?,
                Operation::PostEval => {
                    let f = self.posteval_stack.pop().unwrap();
                    let peek: Option<&T::Ptr> = self.val_stack.last();
                    f(peek);
                    0
                }
            };
            if cost > max_cost && max_cost > 0 {
                let max_cost: Number = max_cost.into();
                let ptr = ptr_from_number(self.allocator, &max_cost)?;
                let n: Node<T> = Node::new(self.allocator, ptr);
                return n.err("cost exceeded");
            }
        }

        Ok(Reduction(cost, self.pop()?))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_program<T: Allocator>(
    allocator: &mut T,
    program: &T::Ptr,
    args: &T::Ptr,
    quote_kw: u8,
    apply_kw: u8,
    max_cost: Cost,
    operator_lookup: Box<dyn OperatorHandler<T>>,
    pre_eval: Option<PreEval<T>>,
) -> Response<T::Ptr>
where
    <T as Allocator>::Ptr: 'static,
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
    use crate::int_allocator::IntAllocator;

    let mut a = IntAllocator::new();
    let nul = a.null();
    let n1 = a.new_atom(&[0, 1, 2]).unwrap();
    let n2 = a.new_atom(&[4, 5, 6]).unwrap();

    assert_eq!(traverse_path(&a, &[0], &n1).unwrap(), Reduction(2, nul));
    assert_eq!(traverse_path(&a, &[0b1], &n1).unwrap(), Reduction(1, n1));
    assert_eq!(traverse_path(&a, &[0b1], &n2).unwrap(), Reduction(1, n2));

    // cost for leading zeros
    assert_eq!(
        traverse_path(&a, &[0, 0, 0, 0], &n1).unwrap(),
        Reduction(5, nul)
    );

    let n3 = a.new_pair(n1, n2).unwrap();
    assert_eq!(traverse_path(&a, &[0b1], &n3).unwrap(), Reduction(1, n3));
    assert_eq!(traverse_path(&a, &[0b10], &n3).unwrap(), Reduction(2, n1));
    assert_eq!(traverse_path(&a, &[0b11], &n3).unwrap(), Reduction(2, n2));
    assert_eq!(traverse_path(&a, &[0b11], &n3).unwrap(), Reduction(2, n2));

    let list = a.new_pair(n1, nul).unwrap();
    let list = a.new_pair(n2, list).unwrap();

    assert_eq!(traverse_path(&a, &[0b10], &list).unwrap(), Reduction(2, n2));
    assert_eq!(
        traverse_path(&a, &[0b101], &list).unwrap(),
        Reduction(3, n1)
    );
    assert_eq!(
        traverse_path(&a, &[0b111], &list).unwrap(),
        Reduction(3, nul)
    );

    // errors
    assert_eq!(
        traverse_path(&a, &[0b1011], &list).unwrap_err(),
        EvalErr(nul.clone(), "path into atom".to_string())
    );
    assert_eq!(
        traverse_path(&a, &[0b1101], &list).unwrap_err(),
        EvalErr(n1.clone(), "path into atom".to_string())
    );
    assert_eq!(
        traverse_path(&a, &[0b1001], &list).unwrap_err(),
        EvalErr(n1.clone(), "path into atom".to_string())
    );
    assert_eq!(
        traverse_path(&a, &[0b1010], &list).unwrap_err(),
        EvalErr(n2.clone(), "path into atom".to_string())
    );
    assert_eq!(
        traverse_path(&a, &[0b1110], &list).unwrap_err(),
        EvalErr(n2.clone(), "path into atom".to_string())
    );
}
