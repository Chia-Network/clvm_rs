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

mod de;
mod ser;
mod ser_optimized;
mod varint;

#[cfg(test)]
mod tests;

/// Magic prefix bytes for serde_2026 format.
///
/// - `0xfd 0xff` forces legacy/backref decoders down an invalid atom-length
///   path (fail-fast).
/// - `0x32 0x30 0x32 0x36` is ASCII `"2026"` for readable hexdumps.
pub const MAGIC_PREFIX: [u8; 6] = [0xfd, 0xff, b'2', b'0', b'2', b'6'];

/// Maximum atoms/pairs that fit in i32 indices (used by instruction stream).
const MAX_INDEX: usize = i32::MAX as usize;

pub use de::{
    deserialize_2026, deserialize_2026_from_stream, node_from_bytes_auto, DeserializeLimits,
};
pub use ser::{
    node_to_bytes_serde_2026, node_to_bytes_serde_2026_raw, serialize_2026,
    serialize_2026_to_stream,
};
pub use ser_optimized::{serialize_2026_pair_optimized, serialize_2026_pair_optimized_to_stream};
