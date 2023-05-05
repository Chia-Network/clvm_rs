#![no_main]
use clvmr::serde::parse_triples;
use libfuzzer_sys::fuzz_target;
use std::io::Cursor;

fuzz_target!(|data: &[u8]| {
    let mut cursor = Cursor::new(data);
    let _triples = parse_triples(&mut cursor, true);
});
