use std::collections::HashMap;
use std::io::Write;

use crate::allocator::{Allocator, NodePtr, SExp};
use crate::error::{EvalErr, Result};
use crate::serde::{InternedTree, intern_tree};

use super::MAX_INDEX;
use super::strategy::{Direction, LeftFirst, VisitStrategy};
use super::varint::write_varint;

/// Intermediate state after interning and sorting atoms.
///
/// Serialization strategies share this prep work.
pub struct SerializerState {
    pub tree: InternedTree,
    pub sorted_no_nil: Vec<usize>,
    pub atom_remap: HashMap<i32, i32>,
    pub nil_old_idx: Option<i32>,
    pub pairs: Vec<(i32, i32)>,
    pub root_index: i32,
}

/// Count how many times each atom is referenced (by root or as a pair child).
fn atom_ref_counts(
    tree: &InternedTree,
    node_to_index: &impl Fn(NodePtr) -> i32,
    root_index: i32,
) -> Vec<u32> {
    let mut counts = vec![0u32; tree.atoms.len()];
    if root_index >= 0 {
        counts[root_index as usize] += 1;
    }
    for &pair_node in &tree.pairs {
        let (left, right) = match tree.allocator.sexp(pair_node) {
            SExp::Pair(l, r) => (l, r),
            _ => unreachable!(),
        };
        for child in [left, right] {
            let idx = node_to_index(child);
            if idx >= 0 {
                counts[idx as usize] += 1;
            }
        }
    }
    counts
}

/// Sort atoms by (reused-first, frequency desc, shorter first, stable).
/// Returns (sorted indices excluding nil, old->new index remap, nil's old index).
fn sort_atoms(
    tree: &InternedTree,
    ref_counts: &[u32],
) -> (Vec<usize>, HashMap<i32, i32>, Option<i32>) {
    let atom_count = tree.atoms.len();

    let mut sorted: Vec<usize> = (0..atom_count).collect();
    sorted.sort_by(|&a, &b| {
        let a_reused = ref_counts[a] > 1;
        let b_reused = ref_counts[b] > 1;
        b_reused
            .cmp(&a_reused)
            .then_with(|| ref_counts[b].cmp(&ref_counts[a]))
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

    let sorted_no_nil: Vec<usize> = sorted
        .iter()
        .filter(|&&old_idx| Some(old_idx as i32) != nil_old_idx)
        .copied()
        .collect();

    let mut atom_remap = HashMap::with_capacity(sorted_no_nil.len());
    for (new_idx, &old_idx) in sorted_no_nil.iter().enumerate() {
        atom_remap.insert(old_idx as i32, new_idx as i32);
    }

    (sorted_no_nil, atom_remap, nil_old_idx)
}

impl SerializerState {
    pub fn new(allocator: &Allocator, node: NodePtr) -> Result<Self> {
        let tree = intern_tree(allocator, node)?;

        if tree.atoms.len() > MAX_INDEX || tree.pairs.len() > MAX_INDEX {
            return Err(EvalErr::SerializationError);
        }

        let mut atom_to_index = HashMap::with_capacity(tree.atoms.len());
        for (i, &atom) in tree.atoms.iter().enumerate() {
            atom_to_index.insert(atom, i as i32);
        }
        let mut pair_to_index = HashMap::with_capacity(tree.pairs.len());
        for (i, &pair) in tree.pairs.iter().enumerate() {
            pair_to_index.insert(pair, -(i as i32 + 1));
        }

        let node_to_index = |n: NodePtr| -> i32 {
            if let Some(&idx) = atom_to_index.get(&n) {
                idx
            } else {
                pair_to_index[&n]
            }
        };

        let root_index = node_to_index(tree.root);
        let ref_counts = atom_ref_counts(&tree, &node_to_index, root_index);
        let (sorted_no_nil, atom_remap, nil_old_idx) = sort_atoms(&tree, &ref_counts);

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

        Ok(Self {
            tree,
            sorted_no_nil,
            atom_remap,
            nil_old_idx,
            pairs,
            root_index,
        })
    }
}

/// Write the atom table (nil excluded) grouped by contiguous equal lengths.
pub fn write_atom_table<W: Write>(
    writer: &mut W,
    tree: &InternedTree,
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

/// Walk the interned pair tree under `strategy`, emitting an instruction
/// stream. Only the visit order at each pair varies across strategies.
pub fn emit_instructions<S: VisitStrategy>(state: &SerializerState, strategy: &S) -> Vec<i64> {
    if state.tree.pairs.is_empty() {
        let mut instructions = Vec::with_capacity(1);
        if Some(state.root_index) == state.nil_old_idx {
            instructions.push(0);
        } else {
            instructions.push(state.atom_remap[&state.root_index] as i64 + 2);
        }
        return instructions;
    }

    enum Op<C: Copy> {
        Build(i32, C),
        Cons(i32, Direction),
    }

    let mut work_stack: Vec<Op<S::NodeCtx>> =
        vec![Op::Build(state.root_index, strategy.root_ctx(state))];
    let mut construction_order: HashMap<i32, i32> = HashMap::new();
    let mut instructions: Vec<i64> = Vec::new();

    while let Some(op) = work_stack.pop() {
        match op {
            Op::Cons(pi, dir) => {
                instructions.push(dir.cons_opcode());
                construction_order.insert(pi, construction_order.len() as i32);
            }
            Op::Build(idx, ctx) => {
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
                    let (dir, l_ctx, r_ctx) = strategy.decide(state, pi, ctx);
                    work_stack.push(Op::Cons(idx, dir));
                    match dir {
                        Direction::LeftFirst => {
                            work_stack.push(Op::Build(right, r_ctx));
                            work_stack.push(Op::Build(left, l_ctx));
                        }
                        Direction::RightFirst => {
                            work_stack.push(Op::Build(left, l_ctx));
                            work_stack.push(Op::Build(right, r_ctx));
                        }
                    }
                }
            }
        }
    }

    instructions
}

/// Write `state` to `writer` using `strategy` for pair visit order.
pub fn serialize_with_strategy<W: Write, S: VisitStrategy>(
    state: &SerializerState,
    strategy: &S,
    writer: &mut W,
) -> Result<()> {
    write_atom_table(writer, &state.tree, &state.sorted_no_nil)?;
    let instructions = emit_instructions(state, strategy);
    write_varint(writer, instructions.len() as i64)?;
    for inst in instructions {
        write_varint(writer, inst)?;
    }
    Ok(())
}

// --- Public entry points ---

use super::Compression;

/// Map a `level: u32` onto an internal compression variant.
///
/// Levels above the highest implemented level are *saturated* down to it,
/// so callers can pass `u32::MAX` to mean "best available compression"
/// without recompiling when new levels are added.
fn compression_for_level(_level: u32) -> Compression {
    // Currently only level 0 (Fast) is implemented; every higher level
    // saturates to it. As new levels land this `match` grows.
    Compression::Fast
}

fn serialize_with_compression<W: Write>(
    allocator: &Allocator,
    node: NodePtr,
    compression: Compression,
    writer: &mut W,
) -> Result<()> {
    let state = SerializerState::new(allocator, node)?;
    match compression {
        Compression::Fast => serialize_with_strategy(&state, &LeftFirst, writer),
    }
}

/// Serialize a CLVM node to the 2026 format at compression `level` with magic prefix.
///
/// This is the primary, recommended wire format. To serialize only the body (without prefix),
/// use [`serialize_2026_body`].
///
/// Levels above the highest implemented level saturate to it, so passing
/// `u32::MAX` always selects the best available compression. Currently
/// only level 0 (left-first / fast) is implemented.
pub fn serialize_2026_to_stream<W: Write>(
    allocator: &Allocator,
    node: NodePtr,
    level: u32,
    writer: &mut W,
) -> Result<()> {
    writer.write_all(&super::SERDE_2026_MAGIC_PREFIX)?;
    serialize_with_compression(allocator, node, compression_for_level(level), writer)
}

/// Serialize a node using the 2026 format at compression `level` with magic prefix, returning bytes.
///
/// This is the primary, recommended wire format. To serialize only the body (without prefix),
/// use [`serialize_2026_body`].
///
/// See [`serialize_2026_to_stream`] for the level-saturation contract.
pub fn serialize_2026(allocator: &Allocator, node: NodePtr, level: u32) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    serialize_2026_to_stream(allocator, node, level, &mut output)?;
    Ok(output)
}

/// Serialize a node using the 2026 format (body only, no magic prefix) at compression `level` to a stream.
///
/// To serialize with the magic prefix (recommended), use [`serialize_2026_to_stream`].
pub fn serialize_2026_body_to_stream<W: Write>(
    allocator: &Allocator,
    node: NodePtr,
    level: u32,
    writer: &mut W,
) -> Result<()> {
    serialize_with_compression(allocator, node, compression_for_level(level), writer)
}
