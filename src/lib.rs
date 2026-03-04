pub mod allocator;
pub mod bls_ops;
pub mod chia_dialect;
pub mod core_ops;
pub mod cost;
pub mod dialect;
pub mod error;
pub mod f_table;
pub mod keccak256_ops;
pub mod more_ops;
pub mod number;
pub mod op_utils;
pub mod reduction;
pub mod run_program;
pub mod runtime_dialect;
pub mod secp_ops;
pub mod serde;
pub mod sha_tree_op;
pub mod traverse_path;
pub mod treehash;

pub use allocator::{Allocator, Atom, NodePtr, ObjectType, SExp};
pub use chia_dialect::ChiaDialect;
pub use run_program::run_program;

pub use chia_dialect::{ClvmFlags, MEMPOOL_MODE};

#[cfg(feature = "counters")]
pub use run_program::run_program_with_counters;

#[cfg(feature = "pre-eval")]
pub use run_program::run_program_with_pre_eval;

#[cfg(feature = "counters")]
pub use run_program::Counters;

// rstest only detects alternative test attributes whose last path segment is
// `test`. Re-exporting the right test macro under that name lets us write
// `#[crate::wasm_compat::test]` on rstest functions and have it resolve to
// `#[wasm_bindgen_test]` on wasm32 or the standard `#[test]` everywhere else.
#[cfg(test)]
pub(crate) mod wasm_compat {
    #[cfg(target_arch = "wasm32")]
    pub use wasm_bindgen_test::wasm_bindgen_test as test;

    #[cfg(not(target_arch = "wasm32"))]
    pub use std::prelude::v1::test;
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod test_ops;
