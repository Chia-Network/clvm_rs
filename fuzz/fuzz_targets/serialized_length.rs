#![no_main]
use clvmr::Allocator;
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::serde::node_to_bytes;
use clvmr::serde::serialized_length_from_bytes;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let len = serialized_length_from_bytes(data);

    let mut allocator = Allocator::new();
    let program = node_from_bytes_backrefs(&mut allocator, data);

    match (len, program) {
        (Ok(_), Ok(_)) => {
            // this is expected
        }
        (Err(_), Err(_)) => {
            // this is expected
        }
        (Ok(len), Err(e)) => {
            panic!(
                "discrepancy between serialized_length and node_from_bytes_backrefs().\n {len}\n{e}"
            );
        }
        (Err(e), Ok(program)) => {
            panic!(
                "discrepancy between serialized_length and node_from_bytes_backrefs().\n {e}\n{:?}",
                node_to_bytes(&allocator, program)
            );
        }
    }
});
