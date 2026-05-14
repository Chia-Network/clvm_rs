#![no_main]

//! Panic-finder for the serde_2026 entry points.
//!
//! Companion to `serde_2026.rs` (which proves *correctness* via roundtrip on
//! valid inputs) and `serde_2026_varint.rs` (which targets the varint codec).
//! This target proves *robustness*: feed every entry point arbitrary bytes
//! under both `strict={false,true}` and assert only that none of them panic,
//! abort, or stack-overflow. `Result::Err` is fine; `Ok` is fine. A SIGABRT
//! from `handle_alloc_error`, an unwinding panic, or a stack overflow is a
//! bug.
//!
//! This deliberately drops the `Corpus::Reject` filter that
//! `serde_2026.rs` uses, so libfuzzer keeps mutating around blobs that fail
//! validation — the historical OOM at `de.rs` (a tiny blob declaring
//! `instruction_count = 2^54`) lives in exactly that "fails validation but
//! aborts before erroring" bucket.

use clvmr::Allocator;
use clvmr::serde_2026::{
    deserialize_2026, deserialize_2026_body_from_stream, serialized_length_serde_2026,
};
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

const FUZZ_MAX_ATOM_LEN: usize = 1 << 20;

fuzz_target!(|data: &[u8]| {
    for strict in [false, true] {
        // Body-only deserializer — slice length is the natural bound.
        let mut a = Allocator::new();
        let _ = deserialize_2026_body_from_stream(
            &mut a,
            &mut Cursor::new(data),
            FUZZ_MAX_ATOM_LEN,
            strict,
        );

        // Prefix-aware deserializer — same body parser, plus magic-prefix strip.
        let mut a = Allocator::new();
        let _ = deserialize_2026(&mut a, data, FUZZ_MAX_ATOM_LEN, strict);

        // Length probe — walks the wire format without building a tree, so
        // it has its own opportunity to allocate or recurse pathologically.
        if let Ok(claimed_length) = serialized_length_serde_2026(data, FUZZ_MAX_ATOM_LEN, strict) {
            // Verify the length function agrees with actual deserialization.
            let mut cursor = Cursor::new(data);
            let mut a = Allocator::new();
            if deserialize_2026_body_from_stream(
                &mut a,
                &mut cursor,
                FUZZ_MAX_ATOM_LEN,
                strict,
            )
            .is_ok()
            {
                let consumed = cursor.position() as usize;
                assert_eq!(
                    claimed_length, consumed,
                    "serialized_length_serde_2026 returned {claimed_length} but deserializer consumed {consumed}"
                );
            }
        }
    }
});
