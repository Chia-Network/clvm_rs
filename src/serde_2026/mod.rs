//! 2026 Serialization Format for CLVM.
//!
//! This module implements a new serialization format designed for efficiency:
//! - Deduplicates atoms and pairs via interning
//! - Uses variable-length integer encoding (varints)
//! - Groups atoms by length for better compression
//! - Uses stack-based instruction stream for tree reconstruction
//!
//! ## Format Overview
//!
//! The serialized format consists of:
//! 1. Atom table: grouped by length, with varint-encoded counts
//! 2. Instruction stream: stack-based operations to reconstruct the tree
//!
//! ## Instructions
//!
//! - Positive varint N: Push atom at index N-1
//! - Zero: Pop two items, cons them, push result
//! - Negative varint -N: Push already-constructed pair at index N-1

mod varint;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};

use crate::allocator::{Allocator, NodePtr, SExp};
use crate::error::{EvalErr, Result};
use crate::serde::intern;
use crate::serde::{node_from_bytes_backrefs};

use varint::{decode_varint, write_varint};

/// Magic prefix bytes for serde_2026 format: the serialization of `(20 . 26)` in
/// classic CLVM.  This is `0xff` (pair marker) followed by atom `20` (`0x14`) and
/// atom `26` (`0x1a`).  No valid real-world CLVM generator starts with this
/// sequence, so it is used as an unambiguous format discriminator.
pub const MAGIC_PREFIX: [u8; 3] = [0xff, 0x14, 0x1a];

/// Maximum atoms/pairs that fit in i32 indices (used by instruction stream).
const MAX_INDEX: usize = i32::MAX as usize;

/// Serialize a node to a stream using the 2026 serialization format.
///
/// This function:
/// 1. Interns the node to deduplicate atoms and pairs
/// 2. Renumbers atoms and pairs for optimal compression
/// 3. Serializes using the 2026 format with varints
pub fn serialize_2026_to_stream<W: Write>(
    allocator: &Allocator,
    node: NodePtr,
    writer: &mut W,
) -> Result<()> {
    // Step 1: Intern the node (single pass)
    let tree = intern(allocator, node)?;

    if tree.atoms.len() > MAX_INDEX || tree.pairs.len() > MAX_INDEX {
        return Err(EvalErr::SerializationError);
    }

    // Step 2: Build node-to-index mappings
    // Atoms: 0, 1, 2, ... (non-negative)
    // Pairs: -1, -2, -3, ... (negative, 1-based)
    let (atom_to_index, pair_to_index) = tree.node_indices();

    let atom_count = tree.atoms.len();
    let pair_count = tree.pairs.len();

    // Combined node-to-index for lookups
    let node_to_index = |n: NodePtr| -> i32 {
        if let Some(&idx) = atom_to_index.get(&n) {
            idx
        } else {
            pair_to_index[&n]
        }
    };

    // Step 3: Sort atoms by length (shorter atoms get lower indices for better varint compression)
    let mut sorted_atom_indices: Vec<usize> = (0..atom_count).collect();
    sorted_atom_indices.sort_by_key(|&i| tree.allocator.atom_len(tree.atoms[i]));

    // Create remap for atoms (maps old index -> new index)
    let mut atom_remap: HashMap<i32, i32> = HashMap::new();
    for (new_idx, &old_idx) in sorted_atom_indices.iter().enumerate() {
        atom_remap.insert(old_idx as i32, new_idx as i32);
    }

    // Step 4: Build remapped pair children (atom indices remapped, pair indices unchanged)
    let remapped_pairs: Vec<(i32, i32)> = tree
        .pairs
        .iter()
        .map(|&pair_node| {
            let (left, right) = match tree.allocator.sexp(pair_node) {
                SExp::Pair(l, r) => (l, r),
                _ => unreachable!(),
            };
            let left_idx = node_to_index(left);
            let right_idx = node_to_index(right);
            let new_left = if left_idx >= 0 {
                atom_remap[&left_idx]
            } else {
                left_idx
            };
            let new_right = if right_idx >= 0 {
                atom_remap[&right_idx]
            } else {
                right_idx
            };
            (new_left, new_right)
        })
        .collect();

    // Step 5: Group atoms by length
    let mut atoms_by_length: HashMap<usize, Vec<NodePtr>> = HashMap::new();
    for &old_idx in &sorted_atom_indices {
        let atom_node = tree.atoms[old_idx];
        let len = tree.allocator.atom_len(atom_node);
        atoms_by_length.entry(len).or_default().push(atom_node);
    }

    // Step 6: Serialize

    // Write number of unique lengths
    write_varint(writer, atoms_by_length.len() as i64)?;

    // Write each length group
    let mut sorted_lengths: Vec<usize> = atoms_by_length.keys().copied().collect();
    sorted_lengths.sort();

    for length in sorted_lengths {
        let atoms_of_length = &atoms_by_length[&length];
        let count = atoms_of_length.len();

        if count == 1 {
            // Single atom: write positive length, then bytes
            write_varint(writer, length as i64)?;
            writer.write_all(tree.allocator.atom(atoms_of_length[0]).as_ref())?;
        } else {
            // Multiple atoms: write negative length, then count, then all bytes
            write_varint(writer, -(length as i64))?;
            write_varint(writer, count as i64)?;
            for &atom_node in atoms_of_length {
                writer.write_all(tree.allocator.atom(atom_node).as_ref())?;
            }
        }
    }

    // Step 7: Generate instruction stream
    let root_index = node_to_index(tree.root);

    if pair_count == 0 {
        // No pairs, root is an atom - just push it
        let remapped_root_idx = atom_remap[&root_index];
        write_varint(writer, 1)?; // One instruction
        write_varint(writer, remapped_root_idx as i64 + 1)?; // Push the atom
    } else {
        // Generate instruction stream using stack-based traversal
        let mut construction_order: HashMap<i32, i32> = HashMap::new();
        let mut instructions: Vec<i64> = Vec::new();

        #[derive(Debug)]
        enum Op {
            Build(i32),
            Cons(i32),
        }

        // Start with the root (root_index is already a pair index like -1, -2, etc.)
        let mut work_stack: Vec<Op> = vec![Op::Build(root_index)];

        while let Some(op) = work_stack.pop() {
            match op {
                Op::Cons(node_index) => {
                    // We've finished processing the children of this pair, record it
                    instructions.push(0); // cons
                    construction_order.insert(node_index, construction_order.len() as i32);
                }
                Op::Build(node_index) => {
                    if node_index >= 0 {
                        // It's an atom, push it (use 1-based indexing)
                        instructions.push(node_index as i64 + 1);
                    } else {
                        // It's a pair
                        // Check if we've already constructed this pair
                        if let Some(&constructed_idx) = construction_order.get(&node_index) {
                            // Reference the already-constructed pair
                            instructions.push(-(constructed_idx as i64 + 1));
                        } else {
                            // Build this pair: process left, process right, then cons
                            let pair_idx = (-node_index - 1) as usize;
                            let (left, right) = remapped_pairs[pair_idx];

                            // Push operations in reverse order
                            work_stack.push(Op::Cons(node_index));
                            work_stack.push(Op::Build(right));
                            work_stack.push(Op::Build(left));
                        }
                    }
                }
            }
        }

        // Write instruction count and instructions
        write_varint(writer, instructions.len() as i64)?;
        for instruction in instructions {
            write_varint(writer, instruction)?;
        }
    }

    Ok(())
}

/// Serialize a node using the 2026 serialization format.
///
/// This function:
/// 1. Interns the node to deduplicate atoms and pairs
/// 2. Renumbers atoms and pairs for optimal compression
/// 3. Serializes using the 2026 format with varints
pub fn serialize_2026(allocator: &Allocator, node: NodePtr) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    serialize_2026_to_stream(allocator, node, &mut output)?;
    Ok(output)
}

/// Serialize a node to serde_2026 format **with** the `(20 . 26)` magic prefix
/// (`ff 14 1a`).  This is the recommended wire format — use `node_from_bytes_auto`
/// to deserialize.
pub fn node_to_bytes_serde_2026(allocator: &Allocator, node: NodePtr) -> Result<Vec<u8>> {
    let raw = serialize_2026(allocator, node)?;
    let mut out = Vec::with_capacity(MAGIC_PREFIX.len() + raw.len());
    out.extend_from_slice(&MAGIC_PREFIX);
    out.extend_from_slice(&raw);
    Ok(out)
}

/// Serialize a node to serde_2026 format **without** the magic prefix.
/// Prefer `node_to_bytes_serde_2026` for wire use; this variant is useful for
/// embedding the payload inside a larger framing that already carries the header.
pub fn node_to_bytes_serde_2026_raw(allocator: &Allocator, node: NodePtr) -> Result<Vec<u8>> {
    serialize_2026(allocator, node)
}

/// Deserialize CLVM from any format (classic, backrefs, or serde_2026).
///
/// Detection logic:
/// 1. If `bytes` starts with `ff 14 1a` (the magic prefix), strip the 3-byte
///    header and deserialize the remainder with `deserialize_2026`.
/// 2. Otherwise delegate to `node_from_bytes_backrefs`, which handles both the
///    classic format and the back-reference extension (backrefs is a strict
///    superset of classic).
///
/// This is the recommended entry point for new code that may encounter any of the
/// three formats.
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
const MAX_COUNT: usize = 256 * 1024 * 1024; // 256M

/// Convert i64 to usize, rejecting negatives and values exceeding max.
fn checked_usize(value: i64, max: usize) -> Result<usize> {
    if value < 0 {
        return Err(EvalErr::SerializationError);
    }
    if value as u64 > usize::MAX as u64 {
        return Err(EvalErr::SerializationError); // doesn't fit in usize (e.g. 32-bit)
    }
    let u = value as usize;
    if u > max {
        return Err(EvalErr::SerializationError);
    }
    Ok(u)
}

/// Deserialize a node from a stream using the 2026 serialization format.
///
/// `max_atom_len` limits the size of any single atom to prevent DoS. Pass `None` to use
/// the default of 1 MiB (2^20 bytes).
pub fn deserialize_2026_from_stream<R: Read>(
    allocator: &mut Allocator,
    reader: &mut R,
    max_atom_len: Option<usize>,
) -> Result<NodePtr> {
    let max_atom_len = max_atom_len.unwrap_or(DEFAULT_MAX_ATOM_LEN);

    // Read atoms - reuse a single buffer for reading
    let mut atoms: Vec<NodePtr> = Vec::new();
    let atom_lengths_count = checked_usize(decode_varint(reader)?, MAX_COUNT)?;
    let mut atom_buffer: Vec<u8> = Vec::new();

    for _ in 0..atom_lengths_count {
        let atom_length = decode_varint(reader)?;
        let (actual_length, atom_count) = if atom_length < 0 {
            if atom_length == i64::MIN {
                return Err(EvalErr::SerializationError); // -i64::MIN overflows
            }
            let len = checked_usize(-atom_length, max_atom_len)?;
            let count = checked_usize(decode_varint(reader)?, MAX_COUNT)?;
            (len, count)
        } else {
            let len = checked_usize(atom_length, max_atom_len)?;
            (len, 1)
        };

        // Resize buffer once per length group
        atom_buffer.resize(actual_length, 0);

        for _ in 0..atom_count {
            reader
                .read_exact(&mut atom_buffer)
                .map_err(|_| EvalErr::SerializationError)?;
            let atom_node = allocator.new_atom(&atom_buffer)?;
            atoms.push(atom_node);
        }
    }

    let instruction_count = checked_usize(decode_varint(reader)?, MAX_COUNT)?;
    if instruction_count == 0 {
        // No pairs, just return the single atom
        return if atoms.is_empty() {
            Err(EvalErr::SerializationError)
        } else {
            Ok(atoms[0])
        };
    }

    // Pre-allocate vectors
    let mut pairs: Vec<NodePtr> = Vec::with_capacity(instruction_count / 2);
    let mut stack: Vec<NodePtr> = Vec::with_capacity(64);

    for _ in 0..instruction_count {
        let instruction = decode_varint(reader)?;
        if instruction == 0 {
            // Cons: pop two items, create pair, push it
            if stack.len() < 2 {
                return Err(EvalErr::SerializationError);
            }
            let right = stack.pop().unwrap();
            let left = stack.pop().unwrap();
            let pair = allocator.new_pair(left, right)?;
            pairs.push(pair);
            stack.push(pair);
        } else if instruction > 0 {
            // Push atom (1-based indexing)
            let atom_idx = (instruction - 1) as usize;
            let atom = *atoms.get(atom_idx).ok_or(EvalErr::SerializationError)?;
            stack.push(atom);
        } else {
            // Push already-constructed pair (negative index)
            let pair_idx = (-instruction - 1) as usize;
            let pair = *pairs.get(pair_idx).ok_or(EvalErr::SerializationError)?;
            stack.push(pair);
        }
    }

    // The final item on the stack is the root
    if stack.len() != 1 {
        return Err(EvalErr::SerializationError);
    }

    Ok(stack[0])
}

/// Deserialize a node from bytes using the 2026 serialization format.
///
/// `max_atom_len` limits the size of any single atom to prevent DoS. Pass `None` to use
/// the default of 1 MiB (2^20 bytes).
pub fn deserialize_2026(
    allocator: &mut Allocator,
    data: &[u8],
    max_atom_len: Option<usize>,
) -> Result<NodePtr> {
    deserialize_2026_from_stream(allocator, &mut Cursor::new(data), max_atom_len)
}
