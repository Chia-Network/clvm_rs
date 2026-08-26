#![no_main]
use clvmr::Allocator;
use clvmr::serde::node_from_bytes_backrefs;
use clvmr::serde::node_to_bytes;
use clvmr::serde::serialized_length_from_bytes;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(len) = serialized_length_from_bytes(data) else {
        // If length fails, a full-buffer parse must also fail.
        let mut allocator = Allocator::new();
        if let Ok(program) = node_from_bytes_backrefs(&mut allocator, data) {
            panic!(
                "discrepancy between serialized_length and node_from_bytes_backrefs().\n Err\n{:?}",
                node_to_bytes(&allocator, program)
            );
        }
        return;
    };

    // serialized_length reports the first complete object and may leave
    // trailing bytes; parse only that prefix so both APIs see the same input.
    let data = &data[..len as usize];
    let mut allocator = Allocator::new();
    node_from_bytes_backrefs(&mut allocator, data).unwrap_or_else(|e| {
        panic!(
            "discrepancy between serialized_length and node_from_bytes_backrefs().\n {len}\n{e}"
        )
    });
});
