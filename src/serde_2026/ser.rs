use std::collections::HashMap;
use std::io::Write;

use crate::allocator::{Allocator, NodePtr, SExp};
use crate::error::{EvalErr, Result};
use crate::serde::{intern_tree, InternedTree};

use super::varint::write_varint;
use super::MAX_INDEX;

/// Intermediate state after interning and sorting atoms.
///
/// Both the default and pair-optimized serializers need this same prep work.
pub(super) struct SerializerState {
    pub tree: InternedTree,
    pub sorted_no_nil: Vec<usize>,
    pub atom_remap: HashMap<i32, i32>,
    pub nil_old_idx: Option<i32>,
    pub pairs: Vec<(i32, i32)>,
    pub root_index: i32,
}

impl SerializerState {
    /// Intern the tree, compute reference counts, sort atoms, build remapping.
    pub fn new(allocator: &Allocator, node: NodePtr) -> Result<Self> {
        let tree = intern_tree(allocator, node)?;

        if tree.atoms.len() > MAX_INDEX || tree.pairs.len() > MAX_INDEX {
            return Err(EvalErr::SerializationError);
        }

        let (atom_to_index, pair_to_index) = tree.node_indices();
        let atom_count = tree.atoms.len();

        let node_to_index = |n: NodePtr| -> i32 {
            if let Some(&idx) = atom_to_index.get(&n) {
                idx
            } else {
                pair_to_index[&n]
            }
        };

        let root_index = node_to_index(tree.root);

        // Reference counts
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

        // Sort: reused-first, then by frequency, then shorter
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

        // Nil gets dedicated opcode 0 — exclude from atom table
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
pub(super) fn write_atom_table<W: Write>(
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

/// Serialize a CLVM node using the 2026 format.
///
/// Atoms are sorted by reference count so frequently-used atoms land in
/// shorter varint buckets. Pairs are traversed left-first (always emits
/// opcode `1`). For pair-ordering optimization, see
/// [`serialize_2026_pair_optimized`](super::serialize_2026_pair_optimized).
pub fn serialize_2026_to_stream<W: Write>(
    allocator: &Allocator,
    node: NodePtr,
    writer: &mut W,
) -> Result<()> {
    let state = SerializerState::new(allocator, node)?;

    write_atom_table(writer, &state.tree, &state.sorted_no_nil)?;

    let pair_count = state.tree.pairs.len();
    if pair_count == 0 {
        write_varint(writer, 1)?;
        if Some(state.root_index) == state.nil_old_idx {
            write_varint(writer, 0)?;
        } else {
            write_varint(writer, state.atom_remap[&state.root_index] as i64 + 2)?;
        }
    } else {
        enum Op {
            Build(i32),
            Cons(i32),
        }

        let mut work_stack: Vec<Op> = vec![Op::Build(state.root_index)];
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
/// This is the recommended wire format — use [`node_from_bytes_auto`](super::node_from_bytes_auto)
/// to deserialize.
pub fn node_to_bytes_serde_2026(allocator: &Allocator, node: NodePtr) -> Result<Vec<u8>> {
    let raw = serialize_2026(allocator, node)?;
    let mut out = Vec::with_capacity(super::MAGIC_PREFIX.len() + raw.len());
    out.extend_from_slice(&super::MAGIC_PREFIX);
    out.extend_from_slice(&raw);
    Ok(out)
}

/// Serialize without the magic prefix (for embedding inside a framing layer
/// that already carries its own header).
pub fn node_to_bytes_serde_2026_raw(allocator: &Allocator, node: NodePtr) -> Result<Vec<u8>> {
    serialize_2026(allocator, node)
}
