#![no_main]

use clvmr::Allocator;
use clvmr::error::EvalErr;
use clvmr::serde::is_canonical_serialization;
use clvmr::serde::node_from_bytes_backrefs;
use libfuzzer_sys::{Corpus, fuzz_target};

fuzz_target!(|data: &[u8]| -> Corpus {
    let mut a = Allocator::new();
    let result = node_from_bytes_backrefs(&mut a, data);
    let canonical = is_canonical_serialization(data);

    match result {
        Ok(_) => {
            // Successful deserialize must be canonical.
            assert!(
                canonical,
                "deserializer accepted a non-canonical serialization"
            );
        }
        Err(EvalErr::NonCanonicalSerialization) => {
            // Explicit non-canonical atom encoding must also fail is_canonical.
            assert!(
                !canonical,
                "is_canonical_serialization accepted a non-canonical atom encoding"
            );
        }
        Err(_) => {
            // Other failures (truncate, bad backref, …) may or may not be
            // considered "canonical" by the byte-level checker; no assertion.
        }
    }
    Corpus::Keep
});
