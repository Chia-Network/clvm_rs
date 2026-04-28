#![no_main]

use clvmr::Allocator;
use clvmr::serde::node_to_bytes;
use clvmr::serde_2026::{
    Compression, DeserializeLimits, deserialize_2026, serialize_2026, serialize_2026_random,
};
use libfuzzer_sys::{Corpus, fuzz_target};

fn canonical(a: &Allocator, node: clvmr::allocator::NodePtr) -> Vec<u8> {
    node_to_bytes(a, node).expect("node_to_bytes failed")
}

fn roundtrip_check(label: &str, a: &Allocator, original: clvmr::allocator::NodePtr, blob: &[u8]) {
    let limits = DeserializeLimits::default();
    let mut a2 = Allocator::new();
    let decoded = deserialize_2026(&mut a2, blob, limits)
        .unwrap_or_else(|e| panic!("{label}: deserialize failed: {e:?}"));
    assert_eq!(
        canonical(a, original),
        canonical(&a2, decoded),
        "{label}: tree mismatch"
    );
}

fuzz_target!(|data: &[u8]| -> Corpus {
    let mut a = Allocator::new();
    let limits = DeserializeLimits::default();

    let Ok(node) = deserialize_2026(&mut a, data, limits) else {
        return Corpus::Reject;
    };

    let fast = serialize_2026(&a, node, Compression::Fast).expect("Fast failed");
    let compact = serialize_2026(&a, node, Compression::Compact).expect("Compact failed");
    let random = serialize_2026_random(&a, node, 0xF022).expect("Random failed");

    roundtrip_check("fast", &a, node, &fast);
    roundtrip_check("compact", &a, node, &compact);
    roundtrip_check("random", &a, node, &random);

    assert!(
        compact.len() <= fast.len(),
        "compact ({}) > fast ({})",
        compact.len(),
        fast.len()
    );

    Corpus::Keep
});
