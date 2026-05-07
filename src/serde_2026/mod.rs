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
//! The current serializer always uses opcode `1` (left-first cons). The format
//! accepts `-1` so future serializers can choose right-first traversal when
//! that helps compression.

mod de;
mod ser;
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

/// Internal vocabulary for which serialization strategy to use. The public API
/// takes a `level: u32` and saturates to the highest implemented level; this
/// enum is the after-saturation result.
///
/// - `Fast` (level 0): left-first traversal. O(N) serialization.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Compression {
    #[default]
    Fast = 0,
}

pub use de::{
    deserialize_2026, deserialize_2026_body_from_stream, deserialize_2026_from_stream,
    serialized_length_serde_2026,
};
pub use ser::{serialize_2026, serialize_2026_body_to_stream, serialize_2026_to_stream};
#[doc(hidden)]
pub use varint::write_varint;
