#![no_main]
use clvmr::serde::tree_hash_from_stream;
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let mut cursor = Cursor::<&[u8]>::new(data);
    let _ = tree_hash_from_stream(&mut cursor);
});
