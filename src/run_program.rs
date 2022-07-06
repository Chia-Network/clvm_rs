use crate::allocator::{Allocator, AtomBuf, NodePtr, SExp};
use crate::cost::Cost;
use crate::dialect::Dialect;
use crate::err_utils::err;
use crate::node::Node;
use crate::reduction::{EvalErr, Reduction, Response};

use crate::number::{ptr_from_number, Number};

// lowered from 46
const QUOTE_COST: Cost = 20;
// lowered from 138
const APPLY_COST: Cost = 90;
// mandatory base cost for every operator we execute
const OP_COST: Cost = 1;

// lowered from measured 147 per bit. It doesn't seem to take this long in
// practice
const TRAVERSE_BASE_COST: Cost = 40;
const TRAVERSE_COST_PER_ZERO_BYTE: Cost = 4;
const TRAVERSE_COST_PER_BIT: Cost = 4;

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
// operator stack (of Operation)

struct RunProgramContext<'a, D> {
    allocator: &'a mut Allocator,
    dialect: &'a D,
    pre_eval: Option<PreEval>,
    posteval_stack: Vec<Box<PostEval>>,
    val_stack: Vec<NodePtr>,
    op_stack: Vec<Operation>,
}

impl<'a, D: Dialect> RunProgramContext<'a, D> {
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
                arg_list = if is_bit_set { right } else { left };
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

impl<'a, D: Dialect> RunProgramContext<'a, D> {
    fn new(allocator: &'a mut Allocator, dialect: &'a D, pre_eval: Option<PreEval>) -> Self {
        RunProgramContext {
            allocator,
            dialect,
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

impl<'a, D: Dialect> RunProgramContext<'a, D> {
    fn eval_op_atom(
        &mut self,
        op_buf: &AtomBuf,
        operator_node: NodePtr,
        operand_list: NodePtr,
        args: NodePtr,
    ) -> Result<Cost, EvalErr> {
        let op_atom = self.allocator.buf(op_buf);
        // special case check for quote
        if op_atom == self.dialect.quote_kw() {
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
            Ok(OP_COST)
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
                    Some(ref pre_eval) => pre_eval(self.allocator, program, args)?,
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
        if op_atom == self.dialect.apply_kw() {
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
                .dialect
                .op(self.allocator, operator, operand_list, max_cost)?;
            self.push(r.1);
            Ok(r.0)
        }
    }

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

pub fn run_program<'a, D: Dialect>(
    allocator: &'a mut Allocator,
    dialect: &'a D,
    program: NodePtr,
    args: NodePtr,
    max_cost: Cost,
    pre_eval: Option<PreEval>,
) -> Response {
    let mut rpc = RunProgramContext::new(allocator, dialect, pre_eval);
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

#[cfg(test)]
struct RunProgramTest {
    prg: &'static str,
    args: &'static str,
    result: Option<&'static str>,
    cost: Cost,
}

#[cfg(test)]
use crate::test_ops::parse_exp;

#[cfg(test)]
const TEST_CASES: &[RunProgramTest] = &[
    // (mod (X N) (defun power (X N) (if (= N 0) 1 (* X (power X (- N 1))))) (power X N))
    RunProgramTest {
        prg: "(a (q 2 2 (c 2 (c 5 (c 11 ())))) (c (q 2 (i (= 11 ()) (q 1 . 1) (q 18 5 (a 2 (c 2 (c 5 (c (- 11 (q . 1)) ())))))) 1) 1))",
        args: "(5033 1000)",
        result: Some("0x024d4f505f1f813ca5e0ae8805bad8707347e65c5f7595da4852be5074288431d1df11a0c326d249f1f52ee051579403d1d0c23a7a1e9af18b7d7dc4c63c73542863c434ae9dfa80141a30cf4acee0d6c896aa2e64ea748404427a3bdaa1b97e4e09b8f5e4f8e9c568a4fc219532dbbad5ec54476d19b7408f8e7e7df16b830c20a1e83d90cc0620b0677b7606307f725539ef223561cdb276baf8e92156ee6492d97159c8f64768349ea7e219fd07fa818a59d81d0563b140396402f0ff758840da19808440e0a57c94c48ef84b4ab7ca8c5f010b69b8f443b12b50bd91bdcf2a96208ddac283fa294d6a99f369d57ab41d03eab5bb4809223c141ad94378516e6766a5054e22e997e260978af68a86893890d612f081b40d54fd1e940af35c0d7900c9a917e2458a61ef8a83f7211f519b2c5f015dfa7c2949ef8bedd02d3bad64ca9b2963dc2bb79f24092331133a7a299872079b9d0422b8fc0eeba4e12c7667ac7282cc6ff98a7c670614c9fce5a061b8d5cd4dd3c6d62d245688b62f9713dc2604bdd5bbc85c070c51f784a9ebac0e0eaa2e29e82d93e570887aa7e1a9d25baf0b2c55a4615f35ec0dbe9baa921569700f95e10cd2d4f6ba152a2ac288c37b60980df33dadfa920fd43dbbf55a0b333b88a3237d954e33d80ed6582019faf51db5f1b52e392559323f8bdd945e7fc6cb8f97f2b8417cfc184d7bfbfa5314d4114f95b725847523f1848d13c28ad96662298ee4e2d87af23e7cb4e58d7a20a5c57ae6833b4a37dcafccca0245a0d6ef28f83200d74db390281e03dd3a8b782970895764c3fcef31c5ed6d0b6e4e796a62ad5654691eea0d9db351cc4fee63248405b24c98bd5e68e4a5e0ab11e90e3c7de270c594d3a35639d931853b7010c8c896f6b28b2af719e53da65da89d44b926b6f06123c9217a43be35d751516bd02c18c4f868a2eae78ae3c6deab1115086c8ce58414db4561865d17ab95c7b3d4e1bfc6d0a4d3fbf5f20a0a7d77a9270e4da354c588da55b0063aec76654019ffb310e1503d99a7bc81ccdf5f8b15c8638156038624cf35988d8420bfdb59184c4b86bf5448df65c44aedc2e98eead7f1ba4be8f402baf12d41076b8f0991cfc778e04ba2c05d1440c70488ffaeefde537064035037f729b683e8ff1b3d0b4aa26a2b30bcaa9379f7fcc7072ff9a2c3e801c5979b0ab3e7acf89373de642d596f26514b9fa213ca217181a8429ad69d14445a822b16818c2509480576dc0ff7bac48c557e6d1883039f4daf873fa4f9a4d849130e2e4336049cfaf9e69a7664f0202b901cf07c7065c4dc93c46f98c5ea5c9c9d911b733093490da3bf1c95f43cd18b7be3798535a55ac6da3442946a268b74bde1349ca9807c41d90c7ec218a17efd2c21d5fcd720501f8a488f1dfba0a423dfdb2a877707b77930e80d734ceabcdb24513fad8f2e2470604d041df083bf184edd0e9720dd2b608b1ee1df951d7ce8ec671317b4f5a3946aa75280658b4ef77b3f504ce73e7ecac84eec3c2b45fb62f6fbd5ab78c744abd3bf5d0ab37d7b19124d2470d53db09ddc1f9dd9654b0e6a3a44c95d0a5a5e061bd24813508d3d1c901544dc3e6b84ca38dd2fde5ea60a57cbc12428848c4e3f6fd4941ebd23d709a717a090dd01830436659f7c20fd2d70c916427e9f3f12ac479128c2783f02a9824aa4e31de133c2704e049a50160f656e28aa0a2615b32bd48bb5d5d13d363a487324c1e9b8703be938bc545654465c9282ad5420978263b3e3ba1bb45e1a382554ac68e5a154b896c9c4c2c3853fbbfc877c4fb7dc164cc420f835c413839481b1d2913a68d206e711fb19b284a7bb2bd2033531647cf135833a0f3026b0c1dc0c184120d30ef4865985fdacdfb848ab963d2ae26a784b7b6a64fdb8feacf94febed72dcd0a41dc12be26ed79af88f1d9cba36ed1f95f2da8e6194800469091d2dfc7b04cfe93ab7a7a888b2695bca45a76a1458d08c3b6176ab89e7edc56c7e01142adfff944641b89cd5703a911145ac4ec42164d90b6fcd78b39602398edcd1f935485894fb8a1f416e031624806f02fbd07f398dbfdd48b86dfacf2045f85ecfe5bb1f01fae758dcdb4ae3b1e2aac6f0878f700d1f430b8ca47c9d8254059bd5c006042c4605f33ca98b41"),
        cost: 15073165,
    },
    // '
    RunProgramTest {
        prg: "(= (point_add (pubkey_for_exp (q . -2)) (pubkey_for_exp (q . 5))) (pubkey_for_exp (q . 3)))",
        args: "()",
        result: Some("1"),
        cost: 6768556,
    },
    RunProgramTest {
        prg: "(= (point_add (pubkey_for_exp (q . 2)) (pubkey_for_exp (q . 3))) (pubkey_for_exp (q . 5)))",
        args: "()",
        result: Some("1"),
        cost: 6768556,
    },
    RunProgramTest {
        prg: "(point_add (pubkey_for_exp (q . 1)) (pubkey_for_exp (q . 2)))",
        args: "()",
        result: Some("0x89ece308f9d1f0131765212deca99697b112d61f9be9a5f1f3780a51335b3ff981747a0b2ca2179b96d2c0c9024e5224"),
        cost: 5442073,
    },
    RunProgramTest {
        prg: "(f (f (q . ((100 200 300) 400 500))))",
        args: "()",
        result: Some("0x64"),
        cost: 82,
    },
    RunProgramTest {
        prg: "(= (f 1) (+ (f (r 1)) (f (r (r 1)))))",
        args: "(7 3 3)",
        result: Some("()"),
        cost: 1194,
    },
    RunProgramTest {
        prg: "(= (f 1) (+ (f (r 1)) (f (r (r 1)))))",
        args: "(7 3 4)",
        result: Some("1"),
        cost: 1194,
    },
    RunProgramTest {
        prg: "(i (f (r (r 1))) (f 1) (f (r 1)))",
        args: "(200 300 400)",
        result: Some("0x00c8"),
        cost: 352,
    },
    RunProgramTest {
        prg: "(i (f (r (r 1))) (f 1) (f (r 1)))",
        args: "(200 300 1)",
        result: Some("0x00c8"),
        cost: 352,
    },
    RunProgramTest {
        prg: "(r (r (q . ((100 200 300) 400 500))))",
        args: "()",
        result: Some("(500)"), // (500)
        cost: 82,
    },
    RunProgramTest {
        prg: "(* (q . 10000000000000000000000000000000000) (q . 10000000000000000000000000000000) (q . 100000000000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000) (q . 1000000000000000000000000000000))",
        args: "()",
        result: Some("0x04261a5c969abab851babdb4f178e63bf2ed3879fc13a4c75622d73c909440a4763849b52e49cd2522500f555f6a3131775f93ddcf24eda7a1dbdf828a033626da873caaaa880a9121f4c44a157973f60443dc53bc99ac12d5bd5fa20a88320ae2ccb8e1b5e792cbf0d001bb0fbd7765d3936e412e2fc8f1267833237237fcb638dda0a7aa674680000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000"),
        cost: 24255,
    },

    // ## APPLY
    RunProgramTest {
        prg: "(a (q 0x0fffffffff) (q ()))",
        args: "()",
        result: None, // invalid operator 0x0fffffffff
        cost: 0,
    },
    RunProgramTest {
        prg: "(a (q . 0) (q . 1) (q . 2))",
        args: "()",
        result: None, // apply requires exactly 2 parameters
        cost: 0,
    },
    RunProgramTest {
        prg: "(a (q 0x00ffffffffffffffffffff00) (q ()))",
        args: "()",
        result: None, // invalid operator 0x00ffffffffffffffffffff00
        cost: 0,
    },
    RunProgramTest {
        prg: "(a (q . 1))",
        args: "()",
        result: None, // apply requires exactly 2 parameters
        cost: 0,
    },
    RunProgramTest {
        prg: "(a (q . 1) (q . (100 200)))",
        args: "()",
        result: Some("(100 200)"),
        cost: 175,
    },
    RunProgramTest {
        prg: "(a (q . (+ 2 5)) (q . (20 30)))",
        args: "()",
        result: Some("50"),
        cost: 987,
    },
    RunProgramTest {
        prg: "((c (q . (+ (q . 50) 1)) (q . 500)))",
        args: "()",
        result: None, // in ((X)...) syntax X must be lone atom
        cost: 0,
    },
    RunProgramTest {
        prg: "((#c) (q . 3) (q . 4))",
        args: "()",
        result: Some("((1 . 3) 1 . 4)"),
        cost: 140,
    },
    RunProgramTest {
        prg: "(a (q . 2) (q . (3 4 5)))",
        args: "()",
        result: Some("3"),
        cost: 179,
    },

    // ## PATH LOOKUPS

    // 0
    RunProgramTest {
        prg: "0",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("()"),
        cost: 44,
    },
    // 1
    RunProgramTest {
        prg: "1",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("(((8 . 12) 10 . 14) (9 . 13) 11 . 15)"),
        cost: 44,
    },
    // 2
    RunProgramTest {
        prg: "2",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("((8 . 12) 10 . 14)"),
        cost: 48,
    },
    // 3
    RunProgramTest {
        prg: "3",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("((9 . 13) 11 . 15)"),
        cost: 48,
    },
    // 4
    RunProgramTest {
        prg: "4",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("(8 . 12)"),
        cost: 52,
    },
    // 5
    RunProgramTest {
        prg: "5",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("(9 . 13)"),
        cost: 52,
    },
    // 6
    RunProgramTest {
        prg: "6",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("(10 . 14)"),
        cost: 52,
    },
    // 7
    RunProgramTest {
        prg: "7",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("(11 . 15)"),
        cost: 52,
    },
    RunProgramTest {
        prg: "8",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("8"),
        cost: 56,
    },
    RunProgramTest {
        prg: "9",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("9"),
        cost: 56,
    },
    RunProgramTest {
        prg: "10",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("10"),
        cost: 56,
    },
    RunProgramTest {
        prg: "11",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("11"),
        cost: 56,
    },
    RunProgramTest {
        prg: "12",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("12"),
        cost: 56,
    },
    RunProgramTest {
        prg: "13",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("13"),
        cost: 56,
    },
    RunProgramTest {
        prg: "14",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("14"),
        cost: 56,
    },
    RunProgramTest {
        prg: "15",
        args: "(((8 . 12) . (10 . 14)) . ((9 . 13) . (11 . 15)))",
        result: Some("15"),
        cost: 56,
    },
    RunProgramTest {
        prg: "0x00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001",
        args: "(((0x1337 . (0x1337 . (42 . 0x1337))) . 0x1337) . 0x1337)",
        result: Some("(((0x1337 . (0x1337 . (42 . 0x1337))) . 0x1337) . 0x1337)"),
        cost: 536,
    },
    RunProgramTest {
        prg: "0x0000C8C141AB3121E776",
        args: "((0x1337 . (0x1337 . ((0x1337 . (0x1337 . (0x1337 . ((0x1337 . (0x1337 . (0x1337 . (((0x1337 . (0x1337 . (0x1337 . (0x1337 . (((((0x1337 . (((0x1337 . ((((0x1337 . (0x1337 . (((0x1337 . (0x1337 . ((0x1337 . ((0x1337 . ((0x1337 . (0x1337 . ((((((0x1337 . ((0x1337 . ((((((0x1337 . (0x1337 . ((((0x1337 . (((0x1337 . 42) . 0x1337) . 0x1337)) . 0x1337) . 0x1337) . 0x1337))) . 0x1337) . 0x1337) . 0x1337) . 0x1337) . 0x1337)) . 0x1337)) . 0x1337) . 0x1337) . 0x1337) . 0x1337) . 0x1337))) . 0x1337)) . 0x1337)) . 0x1337))) . 0x1337) . 0x1337))) . 0x1337) . 0x1337) . 0x1337)) . 0x1337) . 0x1337)) . 0x1337) . 0x1337) . 0x1337) . 0x1337))))) . 0x1337) . 0x1337)))) . 0x1337)))) . 0x1337))) . 0x1337)",
        result: Some("42"),
        cost: 304,
    },
    RunProgramTest {
        prg: "7708975405620101644641102810267383005",
        args: "(0x1337 . ((0x1337 . (0x1337 . (0x1337 . ((0x1337 . (0x1337 . (((0x1337 . ((0x1337 . (0x1337 . (0x1337 . (0x1337 . (0x1337 . ((0x1337 . (0x1337 . ((0x1337 . (((0x1337 . (0x1337 . (0x1337 . ((0x1337 . (((0x1337 . (((0x1337 . (0x1337 . (0x1337 . (0x1337 . ((0x1337 . ((0x1337 . (((((0x1337 . ((0x1337 . ((0x1337 . (0x1337 . (0x1337 . (((0x1337 . (0x1337 . ((0x1337 . (0x1337 . ((((0x1337 . (0x1337 . (0x1337 . (0x1337 . (((((0x1337 . (0x1337 . (0x1337 . (0x1337 . (0x1337 . (((((0x1337 . (((((0x1337 . ((0x1337 . (0x1337 . ((((0x1337 . ((((0x1337 . ((0x1337 . ((0x1337 . ((0x1337 . (0x1337 . (0x1337 . ((((0x1337 . (0x1337 . ((0x1337 . (((0x1337 . (0x1337 . (((0x1337 . (0x1337 . (0x1337 . (42 . 0x1337)))) . 0x1337) . 0x1337))) . 0x1337) . 0x1337)) . 0x1337))) . 0x1337) . 0x1337) . 0x1337)))) . 0x1337)) . 0x1337)) . 0x1337)) . 0x1337) . 0x1337) . 0x1337)) . 0x1337) . 0x1337) . 0x1337))) . 0x1337)) . 0x1337) . 0x1337) . 0x1337) . 0x1337)) . 0x1337) . 0x1337) . 0x1337) . 0x1337)))))) . 0x1337) . 0x1337) . 0x1337) . 0x1337))))) . 0x1337) . 0x1337) . 0x1337))) . 0x1337))) . 0x1337) . 0x1337)))) . 0x1337)) . 0x1337)) . 0x1337) . 0x1337) . 0x1337) . 0x1337)) . 0x1337)) . 0x1337))))) . 0x1337) . 0x1337)) . 0x1337) . 0x1337)) . 0x1337)))) . 0x1337) . 0x1337)) . 0x1337))) . 0x1337)))))) . 0x1337)) . 0x1337) . 0x1337))) . 0x1337)))) . 0x1337))",
        result: Some("42"),
        cost: 532,
    },
    RunProgramTest {
        prg: "1",
        args: "1",
        result: Some("1"),
        cost: 44,
    },
    RunProgramTest {
        prg: "(> 3 3)",
        args: "()",
        result: None, // Path into atom
        cost: 0,
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

        let program = check(parse_exp(&mut allocator, &t.prg));
        let args = check(parse_exp(&mut allocator, &t.args));
        let expected_result = &t.result.map(|v| check(parse_exp(&mut allocator, v)));

        let dialect = ChiaDialect::new(0);
        println!("prg: {}", t.prg);
        match run_program(&mut allocator, &dialect, program, args, t.cost, None) {
            Ok(Reduction(cost, prg_result)) => {
                assert!(node_eq(&allocator, prg_result, expected_result.unwrap()));
                assert_eq!(cost, t.cost);

                // now, run the same program again but with the cost limit 1 too low, to
                // ensure it fails with the correct error
                let expected_cost_exceeded =
                    run_program(&mut allocator, &dialect, program, args, t.cost - 1, None)
                        .unwrap_err();
                assert_eq!(expected_cost_exceeded.1, "cost exceeded");
            }
            Err(err) => {
                println!("FAILED: {}", err.1);
                assert!(expected_result.is_none());
                assert_eq!(t.cost, 0);
            }
        }
    }
}
