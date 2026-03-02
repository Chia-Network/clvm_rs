#![no_main]

use clvm_fuzzing::build_args;
use clvmr::allocator::Allocator;
use clvmr::cost::Cost;
use clvmr::error::EvalErr;
use clvmr::more_ops::op_modpow;
use clvmr::number::Number;
use libfuzzer_sys::fuzz_target;
use num_bigint::Sign;

const MAX_COST: Cost = 6_000_000_000;

fuzz_target!(|input: (Vec<u8>, Vec<u8>, Vec<u8>)| {
    let base = Number::from_signed_bytes_be(&input.0);
    let exp = Number::from_signed_bytes_be(&input.1);
    let modulus = Number::from_signed_bytes_be(&input.2);

    let mut alloc = Allocator::new();
    let args = build_args(&mut alloc, &[&base, &exp, &modulus]);
    let clvm_result = op_modpow(&mut alloc, args, MAX_COST);

    // num-bigint panics on zero modulus or negative exponent, and
    // cargo-fuzz compiles with panic=abort so catch_unwind cannot
    // intercept those panics. Check preconditions explicitly instead.
    let invalid_input = modulus.sign() == Sign::NoSign || exp.sign() == Sign::Minus;

    match clvm_result {
        Err(EvalErr::CostExceeded) => {}
        Err(_) => {
            assert!(
                invalid_input,
                "modpow({base}, {exp}, {modulus}): CLVM failed but inputs are valid"
            );
        }
        Ok(reduction) => {
            assert!(
                !invalid_input,
                "modpow({base}, {exp}, {modulus}): CLVM succeeded but inputs are invalid"
            );
            let expected = base.modpow(&exp, &modulus);
            assert_eq!(
                alloc.number(reduction.1),
                expected,
                "modpow({base}, {exp}, {modulus}): result mismatch"
            );
        }
    }
});
