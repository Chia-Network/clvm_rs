#![no_main]

use clvmr::Allocator;
use clvmr::serde::is_canonical_serialization;
use clvmr::serde::{node_from_bytes, node_to_bytes};
use libfuzzer_sys::{Corpus, fuzz_target};

fuzz_target!(|data: &[u8]| -> Corpus {
    let mut a = Allocator::new();
    let Ok(node) = node_from_bytes(&mut a, data) else {
        return Corpus::Reject;
    };

    let buffer = node_to_bytes(&a, node).expect("internal error, failed to serialize");
    assert!(is_canonical_serialization(&buffer));
    assert_eq!(buffer, data);
    Corpus::Keep
});
