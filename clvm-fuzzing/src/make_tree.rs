use arbitrary::{Arbitrary, Result, Unstructured};
use chia_bls::{G1Element, G2Element};
use clvmr::{Allocator, NodePtr, SExp};

enum Op {
    Pair(bool),
    SubTree,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Arbitrary)]
pub enum ValueKind {
    Program,
    Bytes32,
    SmallInt,
    G1Point,
    G2Point,
    LargeBytes,
    EnvPath,
    Tree,
    Bool,
    List,
}

#[derive(Clone, Copy, Debug)]
pub enum Arity {
    Nullary,
    Unary(ValueKind),
    Binary(ValueKind, ValueKind),
    Ternary(ValueKind, ValueKind, ValueKind),
    BinaryOrTernary(ValueKind, ValueKind, ValueKind),
    Quaternary(ValueKind, ValueKind, ValueKind, ValueKind),
    Varargs {
        min: usize,
        max: usize,
        arg: ValueKind,
    },
    BlsVerify {
        max_pairs: usize,
        sig: ValueKind,
        pk: ValueKind,
        msg: ValueKind,
    },
    BlsPairingIdentity {
        max_pairs: usize,
        g1: ValueKind,
        g2: ValueKind,
    },
}

#[derive(Clone, Copy, Debug)]
struct OpEntry {
    opcode: u32,
    arity: Arity,
    return_kind: ValueKind,
}

const VARARGS_MAX: usize = 8;
const MAX_BLS_VERIFY_PAIRS: usize = 4;
const MAX_BLS_PAIRING_PAIRS: usize = 4;

const PROGRAM_OPS: &[OpEntry] = &[
    OpEntry {
        opcode: 2,
        arity: Arity::Binary(ValueKind::Program, ValueKind::List), // apply
        return_kind: ValueKind::Tree,
    },
    OpEntry {
        opcode: 3,
        arity: Arity::Ternary(ValueKind::Bool, ValueKind::Program, ValueKind::Program), // if
        return_kind: ValueKind::Program,
    },
    OpEntry {
        opcode: 4,
        arity: Arity::Binary(ValueKind::Tree, ValueKind::Tree), // cons
        return_kind: ValueKind::List,
    },
    OpEntry {
        opcode: 5,
        arity: Arity::Unary(ValueKind::List), // first
        return_kind: ValueKind::Tree,
    },
    OpEntry {
        opcode: 6,
        arity: Arity::Unary(ValueKind::List), // rest
        return_kind: ValueKind::List,
    },
    OpEntry {
        opcode: 7,
        arity: Arity::Unary(ValueKind::Tree), // listp
        return_kind: ValueKind::Bool,
    },
    OpEntry {
        opcode: 8,
        arity: Arity::Unary(ValueKind::Tree), // raise
        return_kind: ValueKind::Tree,
    },
    OpEntry {
        opcode: 9,
        arity: Arity::Binary(ValueKind::LargeBytes, ValueKind::LargeBytes), // eq
        return_kind: ValueKind::Bool,
    },
    OpEntry {
        opcode: 10,
        arity: Arity::Binary(ValueKind::LargeBytes, ValueKind::LargeBytes), // gr_bytes
        return_kind: ValueKind::Bool,
    },
    OpEntry {
        opcode: 11,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::LargeBytes,
        }, // sha256
        return_kind: ValueKind::Bytes32,
    },
    OpEntry {
        opcode: 12,
        arity: Arity::BinaryOrTernary(
            ValueKind::LargeBytes,
            ValueKind::SmallInt,
            ValueKind::SmallInt,
        ), // substr
        return_kind: ValueKind::LargeBytes,
    },
    OpEntry {
        opcode: 13,
        arity: Arity::Unary(ValueKind::LargeBytes), // strlen
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 14,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::LargeBytes,
        }, // concat
        return_kind: ValueKind::LargeBytes,
    },
    OpEntry {
        opcode: 16,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::SmallInt,
        }, // add
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 17,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::SmallInt,
        }, // subtract
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 18,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::SmallInt,
        }, // multiply
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 19,
        arity: Arity::Binary(ValueKind::SmallInt, ValueKind::SmallInt), // div
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 20,
        arity: Arity::Binary(ValueKind::SmallInt, ValueKind::SmallInt), // divmod
        return_kind: ValueKind::List,
    },
    OpEntry {
        opcode: 21,
        arity: Arity::Binary(ValueKind::SmallInt, ValueKind::SmallInt), // gr
        return_kind: ValueKind::Bool,
    },
    OpEntry {
        opcode: 22,
        arity: Arity::Binary(ValueKind::SmallInt, ValueKind::SmallInt), // ash
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 23,
        arity: Arity::Binary(ValueKind::SmallInt, ValueKind::SmallInt), // lsh
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 24,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::SmallInt,
        }, // logand
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 25,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::SmallInt,
        }, // logior
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 26,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::SmallInt,
        }, // logxor
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 27,
        arity: Arity::Unary(ValueKind::SmallInt), // lognot
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 29,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::G1Point,
        }, // point_add
        return_kind: ValueKind::G1Point,
    },
    OpEntry {
        opcode: 30,
        arity: Arity::Unary(ValueKind::SmallInt), // pubkey_for_exp
        return_kind: ValueKind::G1Point,
    },
    OpEntry {
        opcode: 32,
        arity: Arity::Unary(ValueKind::Bool), // not
        return_kind: ValueKind::Bool,
    },
    OpEntry {
        opcode: 33,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::Bool,
        }, // any
        return_kind: ValueKind::Bool,
    },
    OpEntry {
        opcode: 34,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::Bool,
        }, // all
        return_kind: ValueKind::Bool,
    },
    OpEntry {
        opcode: 36,
        arity: Arity::Quaternary(
            ValueKind::SmallInt,
            ValueKind::SmallInt,
            ValueKind::Program,
            ValueKind::Tree,
        ), // softfork
        return_kind: ValueKind::Tree,
    },
    OpEntry {
        opcode: 48,
        arity: Arity::Ternary(ValueKind::Bytes32, ValueKind::Bytes32, ValueKind::SmallInt), // coinid
        return_kind: ValueKind::Bytes32,
    },
    OpEntry {
        opcode: 49,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::G1Point,
        }, // bls_g1_subtract
        return_kind: ValueKind::G1Point,
    },
    OpEntry {
        opcode: 50,
        arity: Arity::Binary(ValueKind::G1Point, ValueKind::SmallInt), // bls_g1_multiply
        return_kind: ValueKind::G1Point,
    },
    OpEntry {
        opcode: 51,
        arity: Arity::Unary(ValueKind::G1Point), // bls_g1_negate
        return_kind: ValueKind::G1Point,
    },
    OpEntry {
        opcode: 52,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::G2Point,
        }, // bls_g2_add
        return_kind: ValueKind::G2Point,
    },
    OpEntry {
        opcode: 53,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::G2Point,
        }, // bls_g2_subtract
        return_kind: ValueKind::G2Point,
    },
    OpEntry {
        opcode: 54,
        arity: Arity::Binary(ValueKind::G2Point, ValueKind::SmallInt), // bls_g2_multiply
        return_kind: ValueKind::G2Point,
    },
    OpEntry {
        opcode: 55,
        arity: Arity::Unary(ValueKind::G2Point), // bls_g2_negate
        return_kind: ValueKind::G2Point,
    },
    OpEntry {
        opcode: 56,
        arity: Arity::Varargs {
            min: 1,
            max: 2,
            arg: ValueKind::LargeBytes,
        }, // bls_map_to_g1
        return_kind: ValueKind::G1Point,
    },
    OpEntry {
        opcode: 57,
        arity: Arity::Varargs {
            min: 1,
            max: 2,
            arg: ValueKind::LargeBytes,
        }, // bls_map_to_g2
        return_kind: ValueKind::G2Point,
    },
    OpEntry {
        opcode: 58,
        arity: Arity::BlsPairingIdentity {
            max_pairs: MAX_BLS_PAIRING_PAIRS,
            g1: ValueKind::G1Point,
            g2: ValueKind::G2Point,
        },
        return_kind: ValueKind::Bool,
    },
    OpEntry {
        opcode: 59,
        arity: Arity::BlsVerify {
            max_pairs: MAX_BLS_VERIFY_PAIRS,
            sig: ValueKind::G2Point,
            pk: ValueKind::G1Point,
            msg: ValueKind::LargeBytes,
        },
        return_kind: ValueKind::Bool,
    },
    OpEntry {
        opcode: 60,
        arity: Arity::Ternary(
            ValueKind::SmallInt,
            ValueKind::SmallInt,
            ValueKind::SmallInt,
        ), // modpow
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 61,
        arity: Arity::Binary(ValueKind::SmallInt, ValueKind::SmallInt), // mod
        return_kind: ValueKind::SmallInt,
    },
    OpEntry {
        opcode: 62,
        arity: Arity::Varargs {
            min: 0,
            max: VARARGS_MAX,
            arg: ValueKind::LargeBytes,
        }, // keccak256
        return_kind: ValueKind::Bytes32,
    },
    OpEntry {
        opcode: 63,
        arity: Arity::Unary(ValueKind::Tree), // sha256_tree
        return_kind: ValueKind::Bytes32,
    },
    OpEntry {
        opcode: 0x13d61f00,
        arity: Arity::Ternary(
            ValueKind::LargeBytes,
            ValueKind::Bytes32,
            ValueKind::LargeBytes,
        ),
        return_kind: ValueKind::Bool,
    },
    OpEntry {
        opcode: 0x1c3a8f00,
        arity: Arity::Ternary(
            ValueKind::LargeBytes,
            ValueKind::Bytes32,
            ValueKind::LargeBytes,
        ),
        return_kind: ValueKind::Bool,
    },
];

#[derive(Arbitrary)]
enum NodeType {
    Pair,
    Bytes,
    U8,
    U16,
    U32,
    Previous,
}

#[derive(Debug)]
pub struct ArbitraryClvmTree<const MAX_NODES: i64 = 600_000, const REUSE_NODES: bool = true> {
    pub allocator: Allocator,
    pub tree: NodePtr,
    pub num_nodes: u32,
}

impl<'a, const MAX_NODES: i64, const REUSE_NODES: bool> Arbitrary<'a>
    for ArbitraryClvmTree<MAX_NODES, REUSE_NODES>
{
    fn arbitrary(u: &mut Unstructured<'a>) -> Result<Self> {
        let mut a = Allocator::new();
        let (tree, num_nodes) =
            make_tree_limits(&mut a, u, MAX_NODES, REUSE_NODES).expect("make_tree");
        Ok(Self {
            allocator: a,
            tree,
            num_nodes,
        })
    }
}

pub fn make_tree(a: &mut Allocator, unstructured: &mut Unstructured<'_>) -> (NodePtr, u32) {
    make_tree_limits(a, unstructured, 600_000, true).expect("out of memory")
}

fn arity_args(
    unstructured: &mut Unstructured<'_>,
    arity: &Arity,
) -> anyhow::Result<Vec<ValueKind>> {
    Ok(match *arity {
        Arity::Nullary => Vec::new(),
        Arity::Unary(kind) => vec![kind],
        Arity::Binary(k0, k1) => vec![k0, k1],
        Arity::Ternary(k0, k1, k2) => vec![k0, k1, k2],
        Arity::BinaryOrTernary(k0, k1, k2) => {
            let argc = unstructured.int_in_range(2..=3)?;
            if argc == 2 {
                vec![k0, k1]
            } else {
                vec![k0, k1, k2]
            }
        }
        Arity::Quaternary(k0, k1, k2, k3) => vec![k0, k1, k2, k3],
        Arity::Varargs { min, max, arg } => {
            let argc = unstructured.int_in_range(min..=max)?;
            vec![arg; argc]
        }
        Arity::BlsVerify {
            max_pairs,
            sig,
            pk,
            msg,
        } => {
            let pairs = unstructured.int_in_range(0..=max_pairs)?;
            let mut args = Vec::with_capacity(1 + pairs * 2);
            args.push(sig);
            for _ in 0..pairs {
                args.push(pk);
                args.push(msg);
            }
            args
        }
        Arity::BlsPairingIdentity { max_pairs, g1, g2 } => {
            let pairs = unstructured.int_in_range(0..=max_pairs)?;
            let mut args = Vec::with_capacity(pairs * 2);
            for _ in 0..pairs {
                args.push(g1);
                args.push(g2);
            }
            args
        }
    })
}

fn pick_op_by_return_kind(
    unstructured: &mut Unstructured<'_>,
    kind: ValueKind,
) -> anyhow::Result<Option<OpEntry>> {
    let mut matches = Vec::new();
    for entry in PROGRAM_OPS {
        if entry.return_kind == kind {
            matches.push(*entry);
        }
    }
    if matches.is_empty() {
        Ok(None)
    } else {
        Ok(Some(*unstructured.choose(&matches)?))
    }
}

fn pick_env_path(
    a: &Allocator,
    unstructured: &mut Unstructured<'_>,
    env: NodePtr,
) -> anyhow::Result<u64> {
    if unstructured.ratio(1, 64)? {
        return Ok(unstructured.arbitrary::<u64>()?);
    }

    let mut stack = Vec::new();
    stack.push((env, 1u64, 0u32));
    let mut chosen: Option<u64> = None;
    let mut count: usize = 0;

    while let Some((node, path, depth)) = stack.pop() {
        count = count.saturating_add(1);
        if unstructured.int_in_range(1..=count)? == 1 {
            chosen = Some(path);
        }

        if depth >= 63 {
            continue;
        }

        if let SExp::Pair(left, right) = a.sexp(node) {
            let base_bits = if depth == 0 {
                0
            } else {
                path & ((1u64 << depth) - 1)
            };
            let next_depth = depth + 1;
            let terminator = 1u64 << next_depth;
            let left_path = base_bits | terminator;
            let right_path = base_bits | (1u64 << depth) | terminator;
            stack.push((left, left_path, next_depth));
            stack.push((right, right_path, next_depth));
        }
    }

    Ok(chosen.unwrap_or_else(|| unstructured.arbitrary::<u64>().unwrap_or(1)))
}

fn make_list_value(
    a: &mut Allocator,
    unstructured: &mut Unstructured<'_>,
    env: NodePtr,
    max_nodes: i64,
) -> anyhow::Result<NodePtr> {
    let length = unstructured.int_in_range(0..=4)?;
    let base_nodes = length as i64;
    let available = (max_nodes - base_nodes).max(0);
    let per_item_budget = if length == 0 {
        0
    } else {
        (available / length as i64).max(0)
    };

    let mut list = a.nil();
    for _ in 0..length {
        let item = make_value(a, unstructured, ValueKind::Tree, env, env, per_item_budget)?;
        list = a.new_pair(item, list)?;
    }
    Ok(list)
}

fn make_literal_value(
    a: &mut Allocator,
    unstructured: &mut Unstructured<'_>,
    kind: ValueKind,
    env: NodePtr,
    inner_env: NodePtr,
    max_nodes: i64,
) -> anyhow::Result<NodePtr> {
    let value = match kind {
        ValueKind::Program => make_clvm_program(a, unstructured, inner_env, max_nodes)?,
        ValueKind::Bytes32 => a.new_atom(unstructured.bytes(32)?)?,
        ValueKind::G1Point => {
            if unstructured.ratio(1, 16)? {
                a.new_atom(unstructured.bytes(48)?)?
            } else {
                a.new_g1(G1Element::arbitrary(unstructured)?)?
            }
        }
        ValueKind::G2Point => {
            if unstructured.ratio(1, 16)? {
                a.new_atom(unstructured.bytes(96)?)?
            } else {
                a.new_g2(G2Element::arbitrary(unstructured)?)?
            }
        }
        ValueKind::LargeBytes => {
            let len = unstructured.int_in_range(64..=128)?;
            a.new_atom(unstructured.bytes(len)?)?
        }
        ValueKind::EnvPath => {
            // paths into the environment should not be quoted, so return early
            // here
            let val = pick_env_path(a, unstructured, env)?;
            return Ok(a.new_number(val.into())?);
        }
        ValueKind::SmallInt => {
            let val: u8 = unstructured.arbitrary()?;
            a.new_number(val.into())?
        }
        ValueKind::Bool => *unstructured.choose(&[a.nil(), a.one()])?,
        ValueKind::List => make_list_value(a, unstructured, env, max_nodes)?,
        ValueKind::Tree => make_tree_limits(a, unstructured, max_nodes, true)?.0,
    };
    let quote = a.one();
    Ok(a.new_pair(quote, value)?)
}

fn make_value(
    a: &mut Allocator,
    unstructured: &mut Unstructured<'_>,
    mut kind: ValueKind,
    env: NodePtr,
    inner_env: NodePtr,
    max_nodes: i64,
) -> anyhow::Result<NodePtr> {
    // some of the time, we pick a random value type, rather than the expected
    if unstructured.ratio(1, 32)? {
        let kind: ValueKind = unstructured.arbitrary()?;
        return make_literal_value(a, unstructured, kind, env, inner_env, max_nodes);
    }
    // Sometimes generate a path into the environment
    if unstructured.ratio(1, 16)? {
        kind = ValueKind::EnvPath;
    }

    // 50% of the time we generate an operator call instead of a literal value
    if unstructured.ratio(1, 2)?
        && let Some(entry) = pick_op_by_return_kind(unstructured, kind)?
    {
        make_program_with_entry(a, unstructured, entry, env, max_nodes)
    } else {
        make_literal_value(a, unstructured, kind, env, inner_env, max_nodes)
    }
}

fn make_program_with_entry(
    a: &mut Allocator,
    unstructured: &mut Unstructured<'_>,
    entry: OpEntry,
    env: NodePtr,
    max_nodes: i64,
) -> anyhow::Result<NodePtr> {
    let args_kinds = arity_args(unstructured, &entry.arity)?;
    let n = args_kinds.len();
    let base_nodes = 2 + n;
    if max_nodes < base_nodes as i64 {
        return Ok(a.one());
    }
    let available = max_nodes - base_nodes as i64;
    let arg_budget = if n == 0 {
        0
    } else {
        (available / n as i64).max(0)
    };

    let apply_like = entry.opcode == 2 || entry.opcode == 36;

    let mut args = NodePtr::NIL;
    for kind in args_kinds.iter().rev() {
        let inner_env = if apply_like && *kind == ValueKind::Program {
            // This is a special case for apply and softfork.
            // When generating the program, we need to use the arguments in
            // this call as its environment
            args
        } else {
            env
        };
        let arg = make_value(a, unstructured, *kind, env, inner_env, arg_budget)?;
        args = a.new_pair(arg, args)?;
    }

    let op_atom = a.new_number(entry.opcode.into())?;
    Ok(a.new_pair(op_atom, args)?)
}

pub fn make_clvm_program(
    a: &mut Allocator,
    unstructured: &mut Unstructured<'_>,
    env: NodePtr,
    max_nodes: i64,
) -> anyhow::Result<NodePtr> {
    if unstructured.ratio(1, 64)? {
        return Ok(make_tree_limits(a, unstructured, max_nodes, false)?.0);
    }

    let entry = *unstructured.choose(PROGRAM_OPS)?;
    make_program_with_entry(a, unstructured, entry, env, max_nodes)
}

/// returns an arbitrary CLVM tree structure and the number of (unique) nodes
/// it's made up of. That's both pairs and atoms.
pub fn make_tree_limits(
    a: &mut Allocator,
    unstructured: &mut Unstructured<'_>,
    mut max_nodes: i64,
    reuse_nodes: bool,
) -> anyhow::Result<(NodePtr, u32)> {
    let mut previous_nodes = Vec::new();
    let mut value_stack = Vec::new();
    let mut op_stack = vec![Op::SubTree];
    // the number of Op::SubTree items on the op_stack
    let mut sub_trees: i64 = 1;
    let mut counter = 0;

    while let Some(op) = op_stack.pop() {
        match op {
            Op::Pair(swap) => {
                let left = value_stack.pop().expect("internal error, empty stack");
                let right = value_stack.pop().expect("internal error, empty stack");
                let pair = if swap {
                    a.new_pair(left, right)?
                } else {
                    a.new_pair(right, left)?
                };
                counter += 1;
                value_stack.push(pair);
                previous_nodes.push(pair);
            }
            Op::SubTree => {
                sub_trees -= 1;
                if unstructured.is_empty() {
                    value_stack.push(NodePtr::NIL);
                    continue;
                }
                match unstructured.arbitrary::<NodeType>() {
                    Err(..) => value_stack.push(NodePtr::NIL),
                    Ok(NodeType::Pair) => {
                        if sub_trees > unstructured.len() as i64 || max_nodes <= 0 {
                            // there isn't much entropy left, don't grow the
                            // tree anymore
                            value_stack.push(if reuse_nodes {
                                *unstructured
                                    .choose(&previous_nodes)
                                    .unwrap_or(&NodePtr::NIL)
                            } else {
                                NodePtr::NIL
                            });
                        } else {
                            // swap left and right arbitrarily, to avoid
                            // having a bias because we build the tree depth
                            // first, until we run out of entropy
                            let swap = unstructured.arbitrary::<bool>() == Ok(true);
                            op_stack.push(Op::Pair(swap));
                            op_stack.push(Op::SubTree);
                            op_stack.push(Op::SubTree);
                            sub_trees += 2;
                            max_nodes -= 2;
                        }
                    }
                    Ok(NodeType::Bytes) => {
                        counter += 1;
                        value_stack.push(match unstructured.arbitrary::<Vec<u8>>() {
                            Err(..) => NodePtr::NIL,
                            Ok(val) => {
                                let node = a.new_atom(&val)?;
                                previous_nodes.push(node);
                                node
                            }
                        });
                    }
                    Ok(NodeType::U8) => {
                        counter += 1;
                        value_stack.push(match unstructured.arbitrary::<u8>() {
                            Err(..) => NodePtr::NIL,
                            Ok(val) => a.new_small_number(val.into())?,
                        });
                    }
                    Ok(NodeType::U16) => {
                        counter += 1;
                        value_stack.push(match unstructured.arbitrary::<u16>() {
                            Err(..) => NodePtr::NIL,
                            Ok(val) => a.new_small_number(val.into())?,
                        });
                    }
                    Ok(NodeType::U32) => {
                        counter += 1;
                        value_stack.push(match unstructured.arbitrary::<u32>() {
                            Err(..) => NodePtr::NIL,
                            Ok(val) => a.new_number(val.into())?,
                        });
                    }
                    Ok(NodeType::Previous) => {
                        value_stack.push(if reuse_nodes {
                            *unstructured
                                .choose(&previous_nodes)
                                .unwrap_or(&NodePtr::NIL)
                        } else {
                            NodePtr::NIL
                        });
                    }
                }
            }
        }
    }
    assert_eq!(value_stack.len(), 1);
    Ok((value_stack.remove(0), counter))
}

pub fn make_list(a: &mut Allocator, unstructured: &mut Unstructured<'_>) -> NodePtr {
    let mut ret = NodePtr::NIL;

    let length = unstructured.arbitrary::<u8>().unwrap_or(0);
    let nil_terminated = unstructured.arbitrary::<bool>().unwrap_or(false);

    for _ in 0..length {
        let value = match unstructured
            .arbitrary::<NodeType>()
            .unwrap_or(NodeType::Previous)
        {
            NodeType::U8 => a
                .new_small_number(unstructured.arbitrary::<u8>().unwrap_or(0).into())
                .unwrap(),

            NodeType::U16 => a
                .new_small_number(unstructured.arbitrary::<u16>().unwrap_or(0).into())
                .unwrap(),

            NodeType::U32 => a
                .new_number(unstructured.arbitrary::<u32>().unwrap_or(0).into())
                .unwrap(),

            NodeType::Bytes => a
                .new_atom(&unstructured.arbitrary::<Vec<u8>>().unwrap_or_default())
                .unwrap(),

            NodeType::Pair => {
                let left = NodePtr::NIL;
                let right = NodePtr::NIL;
                a.new_pair(left, right).unwrap()
            }

            NodeType::Previous => NodePtr::NIL,
        };

        ret = if nil_terminated {
            a.new_pair(value, ret).unwrap()
        } else {
            value
        };
    }

    ret
}
