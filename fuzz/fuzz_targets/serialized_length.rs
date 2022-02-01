#![no_main]
use libfuzzer_sys::fuzz_target;
use clvmr::serialize::serialized_length_from_bytes;

fuzz_target!(|data: &[u8]| {
    let _len = match serialized_length_from_bytes(data) {
        Err(_) => { return; },
        Ok(r) => r,
    };
});
