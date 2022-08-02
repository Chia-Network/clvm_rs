#![no_main]
use libfuzzer_sys::fuzz_target;
use clvmr::serialize::tree_hash_from_stream;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {

    let mut cursor = Cursor::<&[u8]>::new(data);
    let _ = tree_hash_from_stream(&mut cursor);
});
