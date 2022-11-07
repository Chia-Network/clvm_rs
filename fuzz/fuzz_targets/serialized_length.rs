#![no_main]
use clvmr::serde::serialized_length_from_bytes;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _len = match serialized_length_from_bytes(data) {
        Err(_) => {
            return;
        }
        Ok(r) => r,
    };
});
