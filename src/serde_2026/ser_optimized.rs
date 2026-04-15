use std::collections::HashMap;
use std::io::Write;

use crate::allocator::{Allocator, NodePtr, SExp};
use crate::error::Result;

use super::ser::{write_atom_table, SerializerState};
use super::varint::write_varint;
use super::MAX_INDEX;

/// Serialize with tree-DP-optimal left/right decisions.
///
/// For each pair, computes the maximum savings achievable by allocating K
/// of its subtree's construction slots to the 1-byte varint tier (63 pair
/// slots). At each pair, compares left-first vs right-first and picks the
/// winner. Complexity: O(N x min(subtree_size, 64)).
///
/// Output is valid for [`deserialize_2026`](super::deserialize_2026) — the
/// deserializer handles both `cons_lr` and `cons_rl` opcodes.
pub fn serialize_2026_pair_optimized_to_stream<W: Write>(
    allocator: &Allocator,
    node: NodePtr,
    writer: &mut W,
) -> Result<()> {
    let state = SerializerState::new(allocator, node)?;
    if state.tree.atoms.len() > MAX_INDEX || state.tree.pairs.len() > MAX_INDEX {
        return Err(crate::error::EvalErr::SerializationError);
    }
    let pair_count = state.tree.pairs.len();

    // Pair reference counts (the shared state only has atom ref counts)
    let mut pair_ref_count = vec![0u32; pair_count];
    if state.root_index < 0 {
        pair_ref_count[(-state.root_index - 1) as usize] += 1;
    }
    for &pair_node in &state.tree.pairs {
        let (left, right) = match state.tree.allocator.sexp(pair_node) {
            SExp::Pair(l, r) => (l, r),
            _ => unreachable!(),
        };
        for child in [left, right] {
            let idx = if let Some(&ai) = state
                .tree
                .node_indices()
                .0
                .get(&child)
            {
                ai
            } else {
                state.tree.node_indices().1[&child]
            };
            if idx < 0 {
                pair_ref_count[(-idx - 1) as usize] += 1;
            }
        }
    }

    // Tree DP: optimal left/right decisions for tier-1 placement
    const TIER1_SLOTS: usize = 63;

    let mut stsize = vec![0usize; pair_count];
    let mut dp: Vec<Vec<u64>> = Vec::with_capacity(pair_count);
    let mut go_left: Vec<Vec<bool>> = Vec::with_capacity(pair_count);

    for (i, &(left, right)) in state.pairs.iter().enumerate() {
        let savings: u64 = if pair_ref_count[i] > 1 {
            (pair_ref_count[i] - 1) as u64
        } else {
            0
        };

        let l_size = if left < 0 {
            stsize[(-left - 1) as usize]
        } else {
            0
        };
        let r_size = if right < 0 {
            stsize[(-right - 1) as usize]
        } else {
            0
        };
        let my_size = 1 + l_size + r_size;
        stsize[i] = my_size;

        let max_k = my_size.min(TIER1_SLOTS + 1);
        let mut my_dp = vec![0u64; max_k + 1];
        let mut my_go = vec![true; max_k + 1];

        let child_dp = |child: i32, budget: usize| -> u64 {
            if child >= 0 {
                return 0;
            }
            let ci = (-child - 1) as usize;
            let arr = &dp[ci];
            arr[budget.min(arr.len() - 1)]
        };

        for k in 1..=max_k {
            let p_cheap = k >= my_size;
            let cb = if p_cheap { k - 1 } else { k };

            let l_lf = cb.min(l_size);
            let r_lf = (cb - l_lf).min(r_size);
            let val_lf = child_dp(left, l_lf) + child_dp(right, r_lf);

            let r_rf = cb.min(r_size);
            let l_rf = (cb - r_rf).min(l_size);
            let val_rf = child_dp(left, l_rf) + child_dp(right, r_rf);

            let p_sav = if p_cheap { savings } else { 0 };
            if val_lf >= val_rf {
                my_dp[k] = p_sav + val_lf;
                my_go[k] = true;
            } else {
                my_dp[k] = p_sav + val_rf;
                my_go[k] = false;
            }
        }

        dp.push(my_dp);
        go_left.push(my_go);
    }

    write_atom_table(writer, &state.tree, &state.sorted_no_nil)?;

    if pair_count == 0 {
        write_varint(writer, 1)?;
        if Some(state.root_index) == state.nil_old_idx {
            write_varint(writer, 0)?;
        } else {
            write_varint(writer, state.atom_remap[&state.root_index] as i64 + 2)?;
        }
    } else {
        enum Op {
            Build(i32, usize),
            ConsLR(i32),
            ConsRL(i32),
        }

        let root_budget = TIER1_SLOTS.min(if state.root_index < 0 {
            stsize[(-state.root_index - 1) as usize]
        } else {
            0
        });

        let mut work_stack: Vec<Op> = vec![Op::Build(state.root_index, root_budget)];
        let mut construction_order: HashMap<i32, i32> = HashMap::new();
        let mut instructions: Vec<i64> = Vec::new();

        while let Some(op) = work_stack.pop() {
            match op {
                Op::ConsLR(pi) => {
                    instructions.push(1);
                    construction_order.insert(pi, construction_order.len() as i32);
                }
                Op::ConsRL(pi) => {
                    instructions.push(-1);
                    construction_order.insert(pi, construction_order.len() as i32);
                }
                Op::Build(idx, budget) => {
                    if idx >= 0 {
                        if Some(idx) == state.nil_old_idx {
                            instructions.push(0);
                        } else {
                            instructions.push(state.atom_remap[&idx] as i64 + 2);
                        }
                    } else if let Some(&ci) = construction_order.get(&idx) {
                        instructions.push(-(ci as i64 + 2));
                    } else {
                        let pi = (-idx - 1) as usize;
                        let (left, right) = state.pairs[pi];
                        let l_size = if left < 0 {
                            stsize[(-left - 1) as usize]
                        } else {
                            0
                        };
                        let r_size = if right < 0 {
                            stsize[(-right - 1) as usize]
                        } else {
                            0
                        };
                        let my_size = 1 + l_size + r_size;

                        let k = budget.min(my_size).min(go_left[pi].len() - 1);
                        let p_cheap = k >= my_size;
                        let cb = if p_cheap { k - 1 } else { k };

                        if go_left[pi][k] {
                            let l_b = cb.min(l_size);
                            let r_b = (cb - l_b).min(r_size);
                            work_stack.push(Op::ConsLR(idx));
                            work_stack.push(Op::Build(right, r_b));
                            work_stack.push(Op::Build(left, l_b));
                        } else {
                            let r_b = cb.min(r_size);
                            let l_b = (cb - r_b).min(l_size);
                            work_stack.push(Op::ConsRL(idx));
                            work_stack.push(Op::Build(left, l_b));
                            work_stack.push(Op::Build(right, r_b));
                        }
                    }
                }
            }
        }

        write_varint(writer, instructions.len() as i64)?;
        for inst in instructions {
            write_varint(writer, inst)?;
        }
    }
    Ok(())
}

/// Serialize with pair-ordering optimization, returning bytes.
pub fn serialize_2026_pair_optimized(allocator: &Allocator, node: NodePtr) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    serialize_2026_pair_optimized_to_stream(allocator, node, &mut output)?;
    Ok(output)
}
