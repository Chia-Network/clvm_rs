#![no_main]

use clvmr::Allocator;
use clvmr::serde::node_to_bytes;
use clvmr::serde_2026::{DeserializeLimits, deserialize_2026, serialize_2026};
use libfuzzer_sys::{Corpus, fuzz_target};

fuzz_target!(|data: &[u8]| -> Corpus {
    let mut a = Allocator::new();
    let limits = DeserializeLimits::default();

    let Ok(node) = deserialize_2026(&mut a, data, limits) else {
        return Corpus::Reject;
    };

    // Re-serialize and deserialize again — must produce identical tree
    let serialized = serialize_2026(&a, node).expect("serialize_2026 failed on valid tree");
    let mut a2 = Allocator::new();
    let node2 =
        deserialize_2026(&mut a2, &serialized, limits).expect("round-trip deserialize failed");

    // Canonical check: both trees must be structurally equal
    let bytes1 = node_to_bytes(&a, node).expect("node_to_bytes failed");
    let bytes2 = node_to_bytes(&a2, node2).expect("node_to_bytes failed");
    assert_eq!(bytes1, bytes2, "round-trip produced different tree");

    Corpus::Keep
});
