#![no_main]

use clvmr::Allocator;
use clvmr::serde::is_canonical_serialization;
use clvmr::serde::{node_from_stream, node_to_bytes};
use libfuzzer_sys::{Corpus, fuzz_target};
use std::io::Cursor;

fuzz_target!(|data: &[u8]| -> Corpus {
    let mut a = Allocator::new();
    let mut cursor = Cursor::<&[u8]>::new(data);
    let Ok(node) = node_from_stream(&mut a, &mut cursor) else {
        return Corpus::Reject;
    };
    let bytes_read = cursor.position();
    let input = &data[0..bytes_read as usize];

    let buffer = node_to_bytes(&a, node).expect("internal error, failed to serialize");
    if is_canonical_serialization(input) {
        assert!(buffer == input);
    } else {
        assert!(buffer != input);
    };
    Corpus::Keep
});
