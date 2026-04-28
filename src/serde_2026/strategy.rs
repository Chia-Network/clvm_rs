//! Visit-order strategies for the 2026 serializer.
//!
//! The serializer walks the interned pair tree and emits a stream of
//! instructions. At each pair, the strategy decides which child to visit
//! first (`LeftFirst` -> opcode `1`, `RightFirst` -> opcode `-1`) and may
//! thread a per-node context forward (e.g. a remaining DP budget).

use super::ser::SerializerState;

/// Which child to visit first at a pair.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Direction {
    LeftFirst,
    RightFirst,
}

impl Direction {
    pub(super) fn cons_opcode(self) -> i64 {
        match self {
            Direction::LeftFirst => 1,
            Direction::RightFirst => -1,
        }
    }
}

/// Pluggable visit-order strategy for `emit_instructions`.
///
/// A strategy may carry its own precomputed tables (e.g. DP results) and
/// thread a per-node context (`NodeCtx`) through the traversal.
pub trait VisitStrategy {
    /// Per-node context propagated down the tree. Use `()` if not needed.
    type NodeCtx: Copy;

    /// Initial context for the root node.
    fn root_ctx(&self, state: &SerializerState) -> Self::NodeCtx;

    /// Decide visit direction at `pair_idx` given the inherited context.
    ///
    /// Returns `(direction, left_ctx, right_ctx)`.
    fn decide(
        &self,
        state: &SerializerState,
        pair_idx: usize,
        ctx: Self::NodeCtx,
    ) -> (Direction, Self::NodeCtx, Self::NodeCtx);
}

/// Always visit the left child first (opcode `1`).
pub struct LeftFirst;

impl VisitStrategy for LeftFirst {
    type NodeCtx = ();

    fn root_ctx(&self, _state: &SerializerState) -> Self::NodeCtx {}

    fn decide(&self, _state: &SerializerState, _pair_idx: usize, _ctx: ()) -> (Direction, (), ()) {
        (Direction::LeftFirst, (), ())
    }
}

/// Pseudorandom visit order. Useful for fuzzing — the output is valid
/// (round-trips through the deserializer) but unlikely to be optimal.
pub struct Random {
    state: std::cell::Cell<u64>,
}

impl Random {
    pub fn new(seed: u64) -> Self {
        let s = if seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            seed
        };
        Self {
            state: std::cell::Cell::new(s),
        }
    }
}

impl VisitStrategy for Random {
    type NodeCtx = ();

    fn root_ctx(&self, _state: &SerializerState) -> Self::NodeCtx {}

    fn decide(&self, _state: &SerializerState, _pair_idx: usize, _ctx: ()) -> (Direction, (), ()) {
        let s = self
            .state
            .get()
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state.set(s);
        let dir = if (s >> 33) & 1 == 0 {
            Direction::LeftFirst
        } else {
            Direction::RightFirst
        };
        (dir, (), ())
    }
}
