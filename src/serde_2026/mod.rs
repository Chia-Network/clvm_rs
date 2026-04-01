//! 2026 Serialization Format for CLVM.
//!
//! Deduplicates atoms and pairs via interning, uses variable-length integer
//! encoding (varints), and groups atoms by length for better compression.
//!
//! ## Format Overview
//!
//! 1. Atom table: grouped by length, with varint-encoded counts (nil excluded)
//! 2. Instruction stream: stack-based operations to reconstruct the tree
//!
//! ## Instructions
//!
//! - `0`: Push nil
//! - `1`: Pop two items (left was pushed first), cons them, push result
//! - `-1`: Pop two items (right was pushed first), cons them, push result
//! - `>= 2` (positive varint N): Push atom at index N-2
//! - `<= -2` (negative varint N): Push already-constructed pair at index -N-2
//!
//! The default serializer always uses opcode `1` (left-first cons). The pair-
//! optimized serializer uses both `1` and `-1` to steer traversal order.

mod varint;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use crate::allocator::{Allocator, NodePtr, SExp};
use crate::error::{EvalErr, Result};
use crate::serde::intern_tree;
use crate::serde::node_from_bytes_backrefs;

use varint::{decode_varint, write_varint};

/// Magic prefix bytes for serde_2026 format.
///
/// - `0xfd 0xff` forces legacy/backref decoders down an invalid atom-length
///   path (fail-fast).
/// - `0x32 0x30 0x32 0x36` is ASCII `"2026"` for readable hexdumps.
pub const MAGIC_PREFIX: [u8; 6] = [0xfd, 0xff, b'2', b'0', b'2', b'6'];

/// Maximum atoms/pairs that fit in i32 indices (used by instruction stream).
const MAX_INDEX: usize = i32::MAX as usize;

// ── Serialization ───────────────────────────────────────────────────────

/// Serialize a CLVM node using the 2026 format.
///
/// Atoms are sorted by reference count so frequently-used atoms land in
/// shorter varint buckets. Pairs are traversed left-first (always emits
/// opcode `1`). For pair-ordering optimization, see
/// [`serialize_2026_pair_optimized`].
pub fn serialize_2026_to_stream<W: Write>(
    allocator: &Allocator,
    node: NodePtr,
    writer: &mut W,
) -> Result<()> {
    let tree = intern_tree(allocator, node)?;

    if tree.atoms.len() > MAX_INDEX || tree.pairs.len() > MAX_INDEX {
        return Err(EvalErr::SerializationError);
    }

    let (atom_to_index, pair_to_index) = tree.node_indices();
    let atom_count = tree.atoms.len();
    let pair_count = tree.pairs.len();

    let node_to_index = |n: NodePtr| -> i32 {
        if let Some(&idx) = atom_to_index.get(&n) {
            idx
        } else {
            pair_to_index[&n]
        }
    };

    let root_index = node_to_index(tree.root);

    // ── atom reference counts (for sorting) ──────────────────────────
    let mut atom_ref_count = vec![0u32; atom_count];
    if root_index >= 0 {
        atom_ref_count[root_index as usize] += 1;
    }
    for &pair_node in &tree.pairs {
        let (left, right) = match tree.allocator.sexp(pair_node) {
            SExp::Pair(l, r) => (l, r),
            _ => unreachable!(),
        };
        for child in [left, right] {
            let idx = node_to_index(child);
            if idx >= 0 {
                atom_ref_count[idx as usize] += 1;
            }
        }
    }

    // ── sort atoms: reused-first, then by frequency, then shorter ────
    let mut sorted_atom_indices: Vec<usize> = (0..atom_count).collect();
    sorted_atom_indices.sort_by(|&a, &b| {
        let a_reused = atom_ref_count[a] > 1;
        let b_reused = atom_ref_count[b] > 1;
        b_reused
            .cmp(&a_reused)
            .then_with(|| atom_ref_count[b].cmp(&atom_ref_count[a]))
            .then_with(|| {
                tree.allocator
                    .atom_len(tree.atoms[a])
                    .cmp(&tree.allocator.atom_len(tree.atoms[b]))
            })
            .then_with(|| a.cmp(&b))
    });

    // nil gets dedicated opcode 0 — exclude from atom table
    let nil_old_idx: Option<i32> = tree
        .atoms
        .iter()
        .position(|&a| tree.allocator.atom_len(a) == 0)
        .map(|i| i as i32);

    let sorted_no_nil: Vec<usize> = sorted_atom_indices
        .iter()
        .filter(|&&old_idx| Some(old_idx as i32) != nil_old_idx)
        .copied()
        .collect();

    let mut atom_remap: HashMap<i32, i32> = HashMap::new();
    for (new_idx, &old_idx) in sorted_no_nil.iter().enumerate() {
        atom_remap.insert(old_idx as i32, new_idx as i32);
    }

    // Pair children in original intern indices (remapped at emit time)
    let pairs: Vec<(i32, i32)> = tree
        .pairs
        .iter()
        .map(|&pair_node| {
            let (left, right) = match tree.allocator.sexp(pair_node) {
                SExp::Pair(l, r) => (l, r),
                _ => unreachable!(),
            };
            (node_to_index(left), node_to_index(right))
        })
        .collect();

    // ── write atom table ─────────────────────────────────────────────
    write_atom_table(writer, &tree, &sorted_no_nil)?;

    // ── instruction stream (always left-first) ───────────────────────
    if pair_count == 0 {
        write_varint(writer, 1)?;
        if Some(root_index) == nil_old_idx {
            write_varint(writer, 0)?;
        } else {
            write_varint(writer, atom_remap[&root_index] as i64 + 2)?;
        }
    } else {
        enum Op {
            Build(i32),
            Cons(i32),
        }

        let mut work_stack: Vec<Op> = vec![Op::Build(root_index)];
        let mut construction_order: HashMap<i32, i32> = HashMap::new();
        let mut instructions: Vec<i64> = Vec::new();

        while let Some(op) = work_stack.pop() {
            match op {
                Op::Cons(pi) => {
                    instructions.push(1);
                    construction_order.insert(pi, construction_order.len() as i32);
                }
                Op::Build(idx) => {
                    if idx >= 0 {
                        if Some(idx) == nil_old_idx {
                            instructions.push(0);
                        } else {
                            instructions.push(atom_remap[&idx] as i64 + 2);
                        }
                    } else if let Some(&ci) = construction_order.get(&idx) {
                        instructions.push(-(ci as i64 + 2));
                    } else {
                        let pi = (-idx - 1) as usize;
                        let (left, right) = pairs[pi];
                        work_stack.push(Op::Cons(idx));
                        work_stack.push(Op::Build(right));
                        work_stack.push(Op::Build(left));
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

/// Serialize a node using the 2026 format, returning bytes.
pub fn serialize_2026(allocator: &Allocator, node: NodePtr) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    serialize_2026_to_stream(allocator, node, &mut output)?;
    Ok(output)
}

/// Serialize with the magic prefix (`fd ff 32 30 32 36`).
/// This is the recommended wire format — use [`node_from_bytes_auto`] to
/// deserialize.
pub fn node_to_bytes_serde_2026(allocator: &Allocator, node: NodePtr) -> Result<Vec<u8>> {
    let raw = serialize_2026(allocator, node)?;
    let mut out = Vec::with_capacity(MAGIC_PREFIX.len() + raw.len());
    out.extend_from_slice(&MAGIC_PREFIX);
    out.extend_from_slice(&raw);
    Ok(out)
}

/// Serialize without the magic prefix (for embedding inside a framing layer
/// that already carries its own header).
pub fn node_to_bytes_serde_2026_raw(allocator: &Allocator, node: NodePtr) -> Result<Vec<u8>> {
    serialize_2026(allocator, node)
}

// ── Deserialization ─────────────────────────────────────────────────────

/// Deserialize CLVM from any format (classic, backrefs, or serde_2026).
///
/// If `bytes` starts with the magic prefix, strip it and deserialize with
/// [`deserialize_2026`]. Otherwise delegate to [`node_from_bytes_backrefs`]
/// (which handles both classic and back-reference formats).
pub fn node_from_bytes_auto(allocator: &mut Allocator, bytes: &[u8]) -> Result<NodePtr> {
    if bytes.starts_with(&MAGIC_PREFIX) {
        deserialize_2026(allocator, &bytes[MAGIC_PREFIX.len()..], None)
    } else {
        node_from_bytes_backrefs(allocator, bytes)
    }
}

/// Default maximum atom length (1 MB) when not specified.
const DEFAULT_MAX_ATOM_LEN: usize = 1 << 20;

/// Maximum number of length groups / instructions to prevent DoS.
const MAX_COUNT: usize = 256 * 1024 * 1024;

fn checked_usize(value: i64, max: usize) -> Result<usize> {
    if value < 0 {
        return Err(EvalErr::SerializationError);
    }
    if value as u64 > usize::MAX as u64 {
        return Err(EvalErr::SerializationError);
    }
    let u = value as usize;
    if u > max {
        return Err(EvalErr::SerializationError);
    }
    Ok(u)
}

/// Deserialize a node from a stream using the 2026 format.
///
/// Handles both `cons_lr` (opcode 1) and `cons_rl` (opcode -1), so it can
/// deserialize output from either [`serialize_2026`] or
/// [`serialize_2026_pair_optimized`].
pub fn deserialize_2026_from_stream<R: Read>(
    allocator: &mut Allocator,
    reader: &mut R,
    max_atom_len: Option<usize>,
) -> Result<NodePtr> {
    let max_atom_len = max_atom_len.unwrap_or(DEFAULT_MAX_ATOM_LEN);

    let mut atoms: Vec<NodePtr> = Vec::new();
    let group_count = checked_usize(decode_varint(reader)?, MAX_COUNT)?;
    let mut buf: Vec<u8> = Vec::new();

    for _ in 0..group_count {
        let length_val = decode_varint(reader)?;
        let (length, count) = if length_val < 0 {
            if length_val == i64::MIN {
                return Err(EvalErr::SerializationError);
            }
            (
                checked_usize(-length_val, max_atom_len)?,
                checked_usize(decode_varint(reader)?, MAX_COUNT)?,
            )
        } else {
            (checked_usize(length_val, max_atom_len)?, 1)
        };
        buf.resize(length, 0);
        for _ in 0..count {
            reader
                .read_exact(&mut buf)
                .map_err(|_| EvalErr::SerializationError)?;
            atoms.push(allocator.new_atom(&buf)?);
        }
    }

    let instruction_count = checked_usize(decode_varint(reader)?, MAX_COUNT)?;
    if instruction_count == 0 {
        return if atoms.is_empty() {
            Err(EvalErr::SerializationError)
        } else {
            Ok(atoms[0])
        };
    }

    let nil = allocator.nil();
    let mut pairs: Vec<NodePtr> = Vec::with_capacity(instruction_count / 3);
    let mut stack: Vec<NodePtr> = Vec::with_capacity(64);

    for _ in 0..instruction_count {
        let inst = decode_varint(reader)?;
        match inst {
            0 => stack.push(nil),
            1 => {
                if stack.len() < 2 {
                    return Err(EvalErr::SerializationError);
                }
                let right = stack.pop().unwrap();
                let left = stack.pop().unwrap();
                let pair = allocator.new_pair(left, right)?;
                pairs.push(pair);
                stack.push(pair);
            }
            -1 => {
                if stack.len() < 2 {
                    return Err(EvalErr::SerializationError);
                }
                let left = stack.pop().unwrap();
                let right = stack.pop().unwrap();
                let pair = allocator.new_pair(left, right)?;
                pairs.push(pair);
                stack.push(pair);
            }
            n if n >= 2 => {
                let ai = (n - 2) as usize;
                stack.push(*atoms.get(ai).ok_or(EvalErr::SerializationError)?);
            }
            n => {
                let pi = (-n - 2) as usize;
                stack.push(*pairs.get(pi).ok_or(EvalErr::SerializationError)?);
            }
        }
    }

    if stack.len() != 1 {
        return Err(EvalErr::SerializationError);
    }
    Ok(stack[0])
}

/// Deserialize a node from bytes using the 2026 format.
pub fn deserialize_2026(
    allocator: &mut Allocator,
    data: &[u8],
    max_atom_len: Option<usize>,
) -> Result<NodePtr> {
    deserialize_2026_from_stream(allocator, &mut Cursor::new(data), max_atom_len)
}

// ── Shared helpers ──────────────────────────────────────────────────────

/// Write the atom table (nil excluded) grouped by contiguous equal lengths.
fn write_atom_table<W: Write>(
    writer: &mut W,
    tree: &crate::serde::InternedTree,
    sorted_no_nil: &[usize],
) -> Result<()> {
    let mut atom_groups: Vec<(usize, Vec<NodePtr>)> = Vec::new();
    for &old_idx in sorted_no_nil {
        let atom_node = tree.atoms[old_idx];
        let len = tree.allocator.atom_len(atom_node);
        if let Some((last_len, atoms)) = atom_groups.last_mut()
            && *last_len == len
        {
            atoms.push(atom_node);
        } else {
            atom_groups.push((len, vec![atom_node]));
        }
    }

    write_varint(writer, atom_groups.len() as i64)?;
    for (length, atoms_of_length) in &atom_groups {
        if atoms_of_length.len() == 1 {
            write_varint(writer, *length as i64)?;
            writer.write_all(tree.allocator.atom(atoms_of_length[0]).as_ref())?;
        } else {
            write_varint(writer, -(*length as i64))?;
            write_varint(writer, atoms_of_length.len() as i64)?;
            for &atom_node in atoms_of_length {
                writer.write_all(tree.allocator.atom(atom_node).as_ref())?;
            }
        }
    }
    Ok(())
}

// ── Pair-optimized serialization (optional) ─────────────────────────────

/// Serialize with tree-DP–optimal left/right decisions.
///
/// For each pair, computes the maximum savings achievable by allocating K
/// of its subtree's construction slots to the 1-byte varint tier (63 pair
/// slots). At each pair, compares left-first vs right-first and picks the
/// winner. Complexity: O(N × min(subtree_size, 64)).
///
/// Output is valid for [`deserialize_2026`] — the deserializer handles both
/// `cons_lr` and `cons_rl` opcodes.
pub fn serialize_2026_pair_optimized_to_stream<W: Write>(
    allocator: &Allocator,
    node: NodePtr,
    writer: &mut W,
) -> Result<()> {
    let tree = intern_tree(allocator, node)?;
    if tree.atoms.len() > MAX_INDEX || tree.pairs.len() > MAX_INDEX {
        return Err(EvalErr::SerializationError);
    }

    let (atom_to_index, pair_to_index) = tree.node_indices();
    let atom_count = tree.atoms.len();
    let pair_count = tree.pairs.len();
    let node_to_index = |n: NodePtr| -> i32 {
        if let Some(&idx) = atom_to_index.get(&n) {
            idx
        } else {
            pair_to_index[&n]
        }
    };

    let root_index = node_to_index(tree.root);

    // ── reference counts (atoms + pairs) ─────────────────────────────
    let mut atom_ref_count = vec![0u32; atom_count];
    let mut pair_ref_count = vec![0u32; pair_count];
    if root_index >= 0 {
        atom_ref_count[root_index as usize] += 1;
    } else {
        pair_ref_count[(-root_index - 1) as usize] += 1;
    }
    for &pair_node in &tree.pairs {
        let (left, right) = match tree.allocator.sexp(pair_node) {
            SExp::Pair(l, r) => (l, r),
            _ => unreachable!(),
        };
        for child in [left, right] {
            let idx = node_to_index(child);
            if idx >= 0 {
                atom_ref_count[idx as usize] += 1;
            } else {
                pair_ref_count[(-idx - 1) as usize] += 1;
            }
        }
    }

    // ── atom sort + nil handling (same as default serializer) ─────────
    let mut sorted_atom_indices: Vec<usize> = (0..atom_count).collect();
    sorted_atom_indices.sort_by(|&a, &b| {
        let a_reused = atom_ref_count[a] > 1;
        let b_reused = atom_ref_count[b] > 1;
        b_reused
            .cmp(&a_reused)
            .then_with(|| atom_ref_count[b].cmp(&atom_ref_count[a]))
            .then_with(|| {
                tree.allocator
                    .atom_len(tree.atoms[a])
                    .cmp(&tree.allocator.atom_len(tree.atoms[b]))
            })
            .then_with(|| a.cmp(&b))
    });

    let nil_old_idx: Option<i32> = tree
        .atoms
        .iter()
        .position(|&a| tree.allocator.atom_len(a) == 0)
        .map(|i| i as i32);

    let sorted_no_nil: Vec<usize> = sorted_atom_indices
        .iter()
        .filter(|&&old_idx| Some(old_idx as i32) != nil_old_idx)
        .copied()
        .collect();

    let mut atom_remap: HashMap<i32, i32> = HashMap::new();
    for (new_idx, &old_idx) in sorted_no_nil.iter().enumerate() {
        atom_remap.insert(old_idx as i32, new_idx as i32);
    }

    let pairs: Vec<(i32, i32)> = tree
        .pairs
        .iter()
        .map(|&pair_node| {
            let (left, right) = match tree.allocator.sexp(pair_node) {
                SExp::Pair(l, r) => (l, r),
                _ => unreachable!(),
            };
            (node_to_index(left), node_to_index(right))
        })
        .collect();

    // ── tree DP: optimal left/right decisions for tier-1 placement ────
    const TIER1_SLOTS: usize = 63; // varint -2…-64

    let mut stsize = vec![0usize; pair_count];
    let mut dp: Vec<Vec<u64>> = Vec::with_capacity(pair_count);
    let mut go_left: Vec<Vec<bool>> = Vec::with_capacity(pair_count);

    for (i, &(left, right)) in pairs.iter().enumerate() {
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

    // ── write atom table ─────────────────────────────────────────────
    write_atom_table(writer, &tree, &sorted_no_nil)?;

    // ── instruction stream guided by DP decisions ────────────────────
    if pair_count == 0 {
        write_varint(writer, 1)?;
        if Some(root_index) == nil_old_idx {
            write_varint(writer, 0)?;
        } else {
            write_varint(writer, atom_remap[&root_index] as i64 + 2)?;
        }
    } else {
        enum Op {
            Build(i32, usize),
            ConsLR(i32),
            ConsRL(i32),
        }

        let root_budget = TIER1_SLOTS.min(if root_index < 0 {
            stsize[(-root_index - 1) as usize]
        } else {
            0
        });

        let mut work_stack: Vec<Op> = vec![Op::Build(root_index, root_budget)];
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
                        if Some(idx) == nil_old_idx {
                            instructions.push(0);
                        } else {
                            instructions.push(atom_remap[&idx] as i64 + 2);
                        }
                    } else if let Some(&ci) = construction_order.get(&idx) {
                        instructions.push(-(ci as i64 + 2));
                    } else {
                        let pi = (-idx - 1) as usize;
                        let (left, right) = pairs[pi];
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
