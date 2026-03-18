#![no_main]

use clvm_fuzzing::build_args;
use clvmr::allocator::{Allocator, NodePtr, SExp};
use clvmr::chia_dialect::ClvmFlags;
use clvmr::cost::Cost;
use clvmr::more_ops::{op_div, op_divmod, op_mod};
use clvmr::number::Number;
use clvmr::reduction::{Reduction, Response};
use libfuzzer_sys::fuzz_target;
use num_bigint::Sign;
use num_integer::Integer;

const MAX_COST: Cost = 6_000_000_000;

type Opf = fn(&mut Allocator, NodePtr, Cost, ClvmFlags) -> Response;

fn canonical_atom_len(n: &Number) -> usize {
    let bytes = n.to_signed_bytes_be();
    let mut slice = bytes.as_slice();
    while !slice.is_empty() && slice[0] == 0 {
        if slice.len() > 1 && (slice[1] & 0x80 == 0x80) {
            break;
        }
        slice = &slice[1..];
    }
    slice.len()
}

fn exceeds_limits(a: &Number, b: &Number, flags: ClvmFlags) -> bool {
    flags.contains(ClvmFlags::LIMITS)
        && (canonical_atom_len(a) > 256 || canonical_atom_len(b) > 1024)
}

fn check_binary_op(
    op: Opf,
    reference: impl Fn(&Number, &Number) -> Number,
    a: &Number,
    b: &Number,
    name: &str,
    flags: ClvmFlags,
) {
    let mut alloc = Allocator::new();
    let args = build_args(&mut alloc, &[a, b]);
    let clvm_result = op(&mut alloc, args, MAX_COST, flags);
    if b.sign() == Sign::NoSign || exceeds_limits(a, b, flags) {
        assert!(
            clvm_result.is_err(),
            "{name}({a}, {b}): CLVM should fail (div_by_zero={}, exceeds_limits={})",
            b.sign() == Sign::NoSign,
            exceeds_limits(a, b, flags)
        );
        return;
    }
    let Reduction(_cost, result) = clvm_result.unwrap_or_else(|e| {
        panic!("{name}({a}, {b}): unexpected error: {e:?}");
    });
    assert_eq!(
        alloc.number(result),
        reference(a, b),
        "{name}({a}, {b}): result mismatch"
    );
}

fn check_divmod(op: Opf, a: &Number, b: &Number, name: &str, flags: ClvmFlags) {
    let mut alloc = Allocator::new();
    let args = build_args(&mut alloc, &[a, b]);
    let clvm_result = op(&mut alloc, args, MAX_COST, flags);
    if b.sign() == Sign::NoSign || exceeds_limits(a, b, flags) {
        assert!(
            clvm_result.is_err(),
            "{name}({a}, {b}): CLVM should fail (div_by_zero={}, exceeds_limits={})",
            b.sign() == Sign::NoSign,
            exceeds_limits(a, b, flags)
        );
        return;
    }
    let Reduction(_cost, result) = clvm_result.unwrap_or_else(|e| {
        panic!("{name}({a}, {b}): unexpected error: {e:?}");
    });
    let (expected_q, expected_r) = a.div_mod_floor(b);
    let SExp::Pair(left, right) = alloc.sexp(result) else {
        panic!("{name}({a}, {b}): result is not a pair");
    };
    assert_eq!(
        alloc.number(left),
        expected_q,
        "{name}({a}, {b}): quotient mismatch"
    );
    assert_eq!(
        alloc.number(right),
        expected_r,
        "{name}({a}, {b}): remainder mismatch"
    );
}

fuzz_target!(|input: (Vec<u8>, Vec<u8>)| {
    let a = Number::from_signed_bytes_be(&input.0);
    let b = Number::from_signed_bytes_be(&input.1);

    for flags in [
        ClvmFlags::empty(),
        ClvmFlags::MALACHITE,
        ClvmFlags::LIMITS,
        ClvmFlags::MALACHITE.union(ClvmFlags::LIMITS),
    ] {
        check_binary_op(op_div, |a, b| a.div_floor(b), &a, &b, "div", flags);
        check_binary_op(op_mod, |a, b| a.mod_floor(b), &a, &b, "mod", flags);
        check_divmod(op_divmod, &a, &b, "divmod", flags);
    }
});
