#![no_main]

use clvmr::Allocator;
use clvmr::serde_2026::{DeserializeLimits, deserialize_2026, serialize_2026};
use libfuzzer_sys::{Corpus, fuzz_target};

fuzz_target!(|data: &[u8]| -> Corpus {
    let mut a = Allocator::new();
    let limits = DeserializeLimits::default();

    let Ok(node) = deserialize_2026(&mut a, data, limits) else {
        return Corpus::Reject;
    };

    // serialize_2026 interns first, so it's safe on DAGs (no exponential blowup)
    let serialized = serialize_2026(&a, node).expect("serialize_2026 failed on valid tree");
    let mut a2 = Allocator::new();
    let node2 =
        deserialize_2026(&mut a2, &serialized, limits).expect("round-trip deserialize failed");
    let serialized2 = serialize_2026(&a2, node2).expect("re-serialize failed");

    assert_eq!(serialized, serialized2, "round-trip produced different serialization");

    Corpus::Keep
});
