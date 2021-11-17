#![no_main]
use libfuzzer_sys::fuzz_target;

use clvm_rs::chia_dialect::chia_dialect;
use clvm_rs::allocator::Allocator;
use clvm_rs::reduction::Reduction;
use clvm_rs::serialize::node_from_bytes;
use clvm_rs::cost::Cost;

fuzz_target!(|data: &[u8]| {
    let mut allocator = Allocator::new();
    let program = match node_from_bytes(&mut allocator, data) {
        Err(_) => { return; },
        Ok(r) => r,
    };
    let args = allocator.null();
    let dialect = chia_dialect(false);

    let Reduction(_cost, _node) = match dialect.run_program(&mut allocator, program, args, 12000000000 as Cost) {
        Err(_) => { return; },
        Ok(r) => r,
    };
});
