use arbitrary::{Arbitrary, Unstructured};
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

#[allow(dead_code)]
pub fn make_tree(a: &mut Allocator, unstructured: &mut Unstructured) -> NodePtr {
    make_tree_limits(a, unstructured, 60_000_000, 60_000_000)
}

pub fn make_tree_limits(
    a: &mut Allocator,
    unstructured: &mut Unstructured,
    mut max_pairs: i64,
    mut max_atoms: i64,
) -> NodePtr {
    let mut previous_nodes = Vec::<NodePtr>::new();
    let mut value_stack = Vec::<NodePtr>::new();
    let mut op_stack = vec![Op::SubTree];
    // the number of Op::SubTree items on the op_stack
    let mut sub_trees: i64 = 1;

    while let Some(op) = op_stack.pop() {
        match op {
            Op::Pair(swap) => {
                let left = value_stack.pop().expect("internal error, empty stack");
                let right = value_stack.pop().expect("internal error, empty stack");
                let pair = if swap {
                    a.new_pair(left, right).expect("out of memory (pair)")
                } else {
                    a.new_pair(right, left).expect("out of memory (pair)")
                };
                value_stack.push(pair);
                previous_nodes.push(pair);
            }
            Op::SubTree => {
                sub_trees -= 1;
                if unstructured.is_empty() {
                    value_stack.push(NodePtr::NIL);
                } else {
                    match unstructured.arbitrary::<NodeType>() {
                        Err(..) => value_stack.push(NodePtr::NIL),
                        Ok(NodeType::Pair) => {
                            if sub_trees > unstructured.len() as i64
                                || max_pairs <= 0
                                || max_atoms <= 0
                            {
                                // there isn't much entropy left, don't grow the
                                // tree anymore
                                value_stack.push(
                                    *unstructured
                                        .choose(&previous_nodes)
                                        .unwrap_or(&NodePtr::NIL),
                                );
                            } else {
                                // swap left and right arbitrarily, to avoid
                                // having a bias because we build the tree depth
                                // first, until we run out of entropy
                                let swap = unstructured.arbitrary::<bool>() == Ok(true);
                                op_stack.push(Op::Pair(swap));
                                op_stack.push(Op::SubTree);
                                op_stack.push(Op::SubTree);
                                sub_trees += 2;
                                max_pairs -= 1;
                                max_atoms -= 2;
                            }
                        }
                        Ok(NodeType::Bytes) => {
                            value_stack.push(match unstructured.arbitrary::<Vec<u8>>() {
                                Err(..) => NodePtr::NIL,
                                Ok(val) => {
                                    let node = a.new_atom(&val).expect("out of memory (atom)");
                                    previous_nodes.push(node);
                                    node
                                }
                            });
                        }
                        Ok(NodeType::U8) => {
                            value_stack.push(match unstructured.arbitrary::<u8>() {
                                Err(..) => NodePtr::NIL,
                                Ok(val) => a
                                    .new_small_number(val.into())
                                    .expect("out of memory (atom)"),
                            });
                        }
                        Ok(NodeType::U16) => {
                            value_stack.push(match unstructured.arbitrary::<u16>() {
                                Err(..) => NodePtr::NIL,
                                Ok(val) => a
                                    .new_small_number(val.into())
                                    .expect("out of memory (atom)"),
                            });
                        }
                        Ok(NodeType::U32) => {
                            value_stack.push(match unstructured.arbitrary::<u32>() {
                                Err(..) => NodePtr::NIL,
                                Ok(val) => a.new_number(val.into()).expect("out of memory (atom)"),
                            });
                        }
                        Ok(NodeType::Previous) => {
                            value_stack.push(
                                *unstructured
                                    .choose(&previous_nodes)
                                    .unwrap_or(&NodePtr::NIL),
                            );
                        }
                    }
                }
            }
        }
    }
    assert_eq!(value_stack.len(), 1);
    *value_stack.last().expect("internal error, empty stack")
}