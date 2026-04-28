mod bitset;
mod bytes32;
mod de;
mod de_br;
mod de_tree;
mod identity_hash;
mod incremental;
mod intern;
mod object_cache;
mod parse_atom;
mod path_builder;
mod read_cache_lookup;
mod ser;
mod ser_br;
mod serialized_length;
mod tools;
mod tree_cache;
mod utils;
pub mod write_atom;

#[cfg(test)]
mod test;
#[cfg(test)]
mod test_intern;

pub use bitset::BitSet;
pub use de::{node_from_bytes, node_from_stream};
pub use de_br::{node_from_bytes_backrefs, node_from_bytes_backrefs_old};
pub use de_tree::{ParsedTriple, parse_triples};
pub use identity_hash::RandomState;
pub use incremental::{Serializer, UndoState};
pub use intern::{InternedTree, intern_tree, intern_tree_limited};
pub use object_cache::{ObjectCache, serialized_length, treehash};
pub use path_builder::{ChildPos, PathBuilder};
pub use read_cache_lookup::ReadCacheLookup;
pub use ser::{node_to_bytes, node_to_bytes_limit};
pub use ser_br::{node_to_bytes_backrefs, node_to_bytes_backrefs_limit};
pub use serialized_length::{serialized_length_atom, serialized_length_small_number};
pub use tools::{
    is_canonical_serialization, serialized_length_from_bytes, serialized_length_from_bytes_trusted,
    tree_hash_from_stream,
};
pub use tree_cache::{TreeCache, TreeCacheCheckpoint};

// Re-export from serde_2026 module for backward compatibility
#[cfg(feature = "ser-2026")]
pub use crate::serde_2026::{
    DeserializeLimits, MAGIC_PREFIX as SERDE_2026_MAGIC_PREFIX, deserialize_2026,
    deserialize_2026_from_stream, node_from_bytes_auto, node_to_bytes_serde_2026,
    node_to_bytes_serde_2026_raw, serialize_2026, serialize_2026_to_stream,
    serialized_length_serde_2026,
};
