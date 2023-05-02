#![no_main]
use std::io::Cursor;
use clvmr::serde::parse_triples;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut cursor = Cursor::new(data);
    let _triples = parse_triples(&mut cursor, true);
});
