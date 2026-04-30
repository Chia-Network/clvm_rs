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
//! The default serializer always uses opcode `1` (left-first cons). The
//! pair-optimized serializer uses both `1` and `-1` to steer traversal order,
//! reducing the number of pair back-references needed.

mod de;
mod ser;
mod ser_optimized;
mod strategy;
mod varint;

#[cfg(test)]
mod tests;

/// Magic prefix bytes for serde_2026 format.
///
/// - `0xfd 0xff` forces legacy/backref decoders down an invalid atom-length
///   path (fail-fast).
/// - `0x32 0x30 0x32 0x36` is ASCII `"2026"` for readable hexdumps.
pub const SERDE_2026_MAGIC_PREFIX: [u8; 6] = [0xfd, 0xff, b'2', b'0', b'2', b'6'];

/// Maximum atoms/pairs that fit in i32 indices (used by instruction stream).
const MAX_INDEX: usize = i32::MAX as usize;

/// Controls the serialization strategy for pair visit order.
///
/// - `Fast` (0): left-first traversal. O(N) serialization.
/// - `Compact` (1): tree-DP to minimize output size by optimizing which
///   pairs land in the 1-byte varint tier. O(N x min(subtree_size, 64)).
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum Compression {
    Fast = 0,
    #[default]
    Compact = 1,
}

pub use de::{
    DeserializeOptions, deserialize_2026, deserialize_2026_from_stream, node_from_bytes_auto,
    serialized_length_serde_2026,
};
pub use ser::{node_to_bytes_serde_2026, serialize_2026, serialize_2026_to_stream};
