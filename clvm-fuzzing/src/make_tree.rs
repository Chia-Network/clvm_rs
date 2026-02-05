use arbitrary::{Arbitrary, Result, Unstructured};
use clvmr::{Allocator, NodePtr};

enum Op {
    Pair(bool),
    SubTree,
}

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
