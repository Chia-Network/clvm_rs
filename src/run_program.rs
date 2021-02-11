use crate::allocator::{Allocator, SExp};
use crate::node::Node;
use crate::reduction::{EvalErr, Reduction, Response};

use crate::number::{ptr_from_number, Number};

const QUOTE_COST: u32 = 1;
const TRAVERSE_COST_PER_ZERO_BYTE: u32 = 1;
const TRAVERSE_COST_PER_BIT: u32 = 1;
const APPLY_COST: u32 = 1;

pub type OperatorHandler<'a, T> =
    Box<dyn 'a + Fn(&T, &[u8], &<T as Allocator>::Ptr) -> Response<<T as Allocator>::Ptr>>;

pub type PreEval<A> = Box<
    dyn Fn(
        &A,
        &<A as Allocator>::Ptr,
        &<A as Allocator>::Ptr,
    ) -> Result<Option<Box<PostEval<A>>>, EvalErr<<A as Allocator>::Ptr>>,
>;

pub type PostEval<T> = dyn Fn(Option<&<T as Allocator>::Ptr>);

type RpcOperator<T> =
    dyn FnOnce(&mut RunProgramContext<T>) -> Result<u32, EvalErr<<T as Allocator>::Ptr>>;

// `run_program` has two stacks: the operand stack (of `Node` objects) and the
// operator stack (of RpcOperators)

pub struct RunProgramContext<'a, 'h, T: Allocator> {
    allocator: &'a T,
    quote_kw: u8,
    apply_kw: u8,
    operator_lookup: &'h OperatorHandler<'a, T>,
    pre_eval: Option<PreEval<T>>,
    val_stack: Vec<T::Ptr>,
    op_stack: Vec<Box<RpcOperator<T>>>,
}

pub fn make_err<T: Clone, V>(node: &T, msg: &str) -> Result<V, EvalErr<T>> {
    Err(EvalErr(node.clone(), msg.into()))
}

impl<'a, 'h, T: Allocator> RunProgramContext<'a, 'h, T> {
    pub fn pop(&mut self) -> Result<T::Ptr, EvalErr<T::Ptr>> {
        let v: Option<T::Ptr> = self.val_stack.pop();
        match v {
            None => {
                let node: T::Ptr = self.allocator.null();
                make_err(&node, "runtime error: value stack empty")
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

    let mut cost: u32 =
        (first_bit_byte_index as u32) * TRAVERSE_COST_PER_ZERO_BYTE + TRAVERSE_COST_PER_BIT;

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

impl<'a, 'h, T: Allocator> RunProgramContext<'a, 'h, T> {
    fn new(
        allocator: &'a T,
        quote_kw: u8,
        apply_kw: u8,
        operator_lookup: &'h OperatorHandler<'a, T>,
        pre_eval: Option<PreEval<T>>,
    ) -> Self {
        RunProgramContext {
            allocator,
            quote_kw,
            apply_kw,
            operator_lookup,
            pre_eval,
            val_stack: Vec::new(),
            op_stack: Vec::new(),
        }
    }

    fn swap_op(&mut self) -> Result<u32, EvalErr<T::Ptr>> {
        /* Swap the top two operands. */
        let v2 = self.pop()?;
        let v1 = self.pop()?;
        self.push(v2);
        self.push(v1);
        Ok(0)
    }

    fn cons_op(&mut self) -> Result<u32, EvalErr<T::Ptr>> {
        /* Join the top two operands. */
        let v1 = self.pop()?;
        let v2 = self.pop()?;
        self.push(self.allocator.new_pair(v1, v2));
        Ok(0)
    }
}

impl<'a, 'h, T: Allocator> RunProgramContext<'a, 'h, T>
where
    <T as Allocator>::Ptr: 'static,
{
    fn eval_op_atom(
        &mut self,
        op_atom: &[u8],
        operator_node: &T::Ptr,
        operand_list: &T::Ptr,
        args: &T::Ptr,
    ) -> Result<u32, EvalErr<T::Ptr>> {
        // special case check for quote
        if op_atom.len() == 1 && op_atom[0] == self.quote_kw {
            match self.allocator.sexp(operand_list) {
                SExp::Atom(_) => make_err(&operand_list, "quote requires exactly 1 parameter"),
                SExp::Pair(quoted_val, nil) => {
                    if Node::new(self.allocator, nil).nullp() {
                        self.push(quoted_val);
                        Ok(QUOTE_COST)
                    } else {
                        make_err(&operand_list, "quote requires exactly 1 parameter")
                    }
                }
            }
        } else {
            self.op_stack.push(Box::new(|r| r.apply_op()));
            self.push(operator_node.clone());
            let mut operands: T::Ptr = operand_list.clone();
            loop {
                if Node::new(self.allocator, operands.clone()).nullp() {
                    break;
                }
                self.op_stack.push(Box::new(|r| r.cons_op()));
                self.op_stack.push(Box::new(|r| r.eval_op()));
                self.op_stack.push(Box::new(|r| r.swap_op()));
                match self.allocator.sexp(&operands) {
                    SExp::Atom(_) => return make_err(operand_list, "bad operand list"),
                    SExp::Pair(first, rest) => {
                        let new_pair = self.allocator.new_pair(first, args.clone());
                        self.push(new_pair);
                        operands = rest.clone();
                    }
                }
            }
            self.push(self.allocator.null());
            Ok(1)
        }
    }

    fn eval_pair(&mut self, program: &T::Ptr, args: &T::Ptr) -> Result<u32, EvalErr<T::Ptr>> {
        // put a bunch of ops on op_stack
        match self.allocator.sexp(program) {
            // the program is just a bitfield path through the args tree
            SExp::Atom(path) => {
                let r: Reduction<T::Ptr> = traverse_path(self.allocator, path, &args)?;
                self.push(r.1);
                Ok(r.0)
            }
            // the program is an operator and a list of operands
            SExp::Pair(operator_node, operand_list) => match self.allocator.sexp(&operator_node) {
                SExp::Pair(_, _) => {
                    // the operator is also a list, so we need two evals here
                    self.push(self.allocator.new_pair(operator_node, args.clone()));
                    self.op_stack.push(Box::new(|r| r.eval_op()));
                    self.op_stack.push(Box::new(|r| r.eval_op()));
                    Ok(1)
                }
                SExp::Atom(op_atom) => {
                    self.eval_op_atom(&op_atom, &operator_node, &operand_list, args)
                }
            },
        }
    }

    fn eval_op(&mut self) -> Result<u32, EvalErr<T::Ptr>> {
        /*
        Pop the top value and treat it as a (program, args) pair, and manipulate
        the op & value stack to evaluate all the arguments and apply the operator.
        */

        let pair: T::Ptr = self.pop()?;
        match self.allocator.sexp(&pair) {
            SExp::Atom(_) => make_err(&pair, "pair expected"),
            SExp::Pair(program, args) => {
                let post_eval = match self.pre_eval {
                    None => None,
                    Some(ref pre_eval) => pre_eval(&self.allocator, &program, &args)?,
                };
                if let Some(post_eval) = post_eval {
                    let new_function: Box<RpcOperator<T>> =
                        Box::new(move |rpc: &mut RunProgramContext<T>| {
                            let peek: Option<&T::Ptr> = rpc.val_stack.last();
                            post_eval(peek);
                            Ok(0)
                        });
                    self.op_stack.push(new_function);
                };
                self.eval_pair(&program, &args)
            }
        }
    }

    fn apply_op(&mut self) -> Result<u32, EvalErr<T::Ptr>> {
        let operand_list = self.pop()?;
        let operator = self.pop()?;
        match self.allocator.sexp(&operator) {
            SExp::Pair(_, _) => Err(EvalErr(operator, "internal error".into())),
            SExp::Atom(op_atom) => {
                if op_atom.len() == 1 && op_atom[0] == self.apply_kw {
                    let operand_list = Node::new(self.allocator, operand_list);
                    if operand_list.arg_count_is(2) {
                        let new_operator = operand_list.first()?;
                        let new_operand_list = operand_list.rest()?.first()?;
                        match new_operator.sexp() {
                            SExp::Pair(_, _) => {
                                let new_pair = self
                                    .allocator
                                    .new_pair(new_operator.node, new_operand_list.node);
                                self.push(new_pair);
                                self.op_stack.push(Box::new(|r| r.eval_op()));
                            }
                            SExp::Atom(_) => {
                                self.push(new_operator.node);
                                self.push(new_operand_list.node);
                                self.op_stack.push(Box::new(|r| r.apply_op()));
                            }
                        };
                        Ok(APPLY_COST)
                    } else {
                        operand_list.err("apply requires exactly 2 parameters")
                    }
                } else {
                    let r = (self.operator_lookup)(self.allocator, &op_atom, &operand_list)?;
                    self.push(r.1);
                    Ok(r.0)
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn run_program(
        &mut self,
        program: &T::Ptr,
        args: &T::Ptr,
        max_cost: u32,
    ) -> Response<T::Ptr> {
        self.val_stack = vec![self.allocator.new_pair(program.clone(), args.clone())];
        self.op_stack = vec![Box::new(|r| r.eval_op())];

        let mut cost: u32 = 0;

        loop {
            let top = self.op_stack.pop();
            match top {
                Some(f) => {
                    cost += f(self)?;
                }
                None => break,
            }
            if cost > max_cost && max_cost > 0 {
                let max_cost: Number = max_cost.into();
                let n: Node<T> =
                    Node::new(self.allocator, ptr_from_number(self.allocator, &max_cost));
                return n.err("cost exceeded");
            }
        }

        Ok(Reduction(cost, self.pop()?))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_program<T: Allocator>(
    allocator: &T,
    program: &T::Ptr,
    args: &T::Ptr,
    quote_kw: u8,
    apply_kw: u8,
    max_cost: u32,
    operator_lookup: &OperatorHandler<T>,
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

    let a = IntAllocator::new();
    let nul = a.null();
    let n1 = a.new_atom(&[0, 1, 2]);
    let n2 = a.new_atom(&[4, 5, 6]);

    assert_eq!(traverse_path(&a, &[0], &n1).unwrap(), Reduction(2, nul));
    assert_eq!(traverse_path(&a, &[0b1], &n1).unwrap(), Reduction(1, n1));
    assert_eq!(traverse_path(&a, &[0b1], &n2).unwrap(), Reduction(1, n2));

    // cost for leading zeros
    assert_eq!(
        traverse_path(&a, &[0, 0, 0, 0], &n1).unwrap(),
        Reduction(5, nul)
    );

    let n3 = a.new_pair(n1, n2);
    assert_eq!(traverse_path(&a, &[0b1], &n3).unwrap(), Reduction(1, n3));
    assert_eq!(traverse_path(&a, &[0b10], &n3).unwrap(), Reduction(2, n1));
    assert_eq!(traverse_path(&a, &[0b11], &n3).unwrap(), Reduction(2, n2));
    assert_eq!(traverse_path(&a, &[0b11], &n3).unwrap(), Reduction(2, n2));

    let list = a.new_pair(n1, nul);
    let list = a.new_pair(n2, list);

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
