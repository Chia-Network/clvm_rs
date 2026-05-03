#![no_main]

use clvmr::serde_2026::{decode_varint, encode_varint};
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    // Non-strict decode must never panic on arbitrary input.
    let mut cur = Cursor::new(data);
    let non_strict = decode_varint(&mut cur, false);

    if let Ok(v) = non_strict {
        let consumed = cur.position() as usize;

        // The canonical (minimal) encoding must:
        //  - decode in strict mode to the same value
        //  - never be longer than the bytes we just consumed
        //    (strict-mode rejections are exactly the "overlong" case)
        let canonical = encode_varint(v);
        assert!(
            canonical.len() <= consumed,
            "canonical encoding of {v} is {} bytes but input took {consumed}",
            canonical.len(),
        );
        let strict_canonical = decode_varint(&mut Cursor::new(&canonical[..]), true)
            .expect("canonical encoding must decode under strict");
        assert_eq!(v, strict_canonical, "canonical roundtrip mismatch");

        // Strict-mode decode of the original input either matches the
        // non-strict value (canonical input) or errors (overlong / non-canonical).
        match decode_varint(&mut Cursor::new(data), true) {
            Ok(strict_v) => assert_eq!(strict_v, v, "strict and non-strict disagree"),
            Err(_) => {
                // Overlong-but-valid-as-non-strict encoding. Confirm it really is
                // non-canonical: the canonical form must be shorter.
                assert!(
                    canonical.len() < consumed,
                    "strict rejected a canonical-length encoding"
                );
            }
        }
    } else {
        // If non-strict failed, strict must also fail (strict is stricter).
        assert!(decode_varint(&mut Cursor::new(data), true).is_err());
    }
});
