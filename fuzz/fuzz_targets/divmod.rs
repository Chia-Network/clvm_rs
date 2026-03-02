#![no_main]

use clvm_fuzzing::build_args;
use clvmr::allocator::{Allocator, NodePtr, SExp};
use clvmr::cost::Cost;
use clvmr::more_ops::{op_div, op_divmod, op_mod};
use clvmr::number::Number;
use clvmr::reduction::{Reduction, Response};
use libfuzzer_sys::fuzz_target;
use num_bigint::Sign;
use num_integer::Integer;

const MAX_COST: Cost = 6_000_000_000;

type Opf = fn(&mut Allocator, NodePtr, Cost) -> Response;

fn check_binary_op(
    op: Opf,
    reference: impl Fn(&Number, &Number) -> Number,
    a: &Number,
    b: &Number,
    name: &str,
) {
    let mut alloc = Allocator::new();
    let args = build_args(&mut alloc, &[a, b]);
    let clvm_result = op(&mut alloc, args, MAX_COST);
    if b.sign() == Sign::NoSign {
        assert!(clvm_result.is_err(), "{name}({a}, 0): CLVM should fail");
        return;
    }
    let Reduction(_cost, result) = clvm_result.expect(name);
    assert_eq!(
        alloc.number(result),
        reference(a, b),
        "{name}({a}, {b}): result mismatch"
    );
}

fn check_divmod(a: &Number, b: &Number) {
    let mut alloc = Allocator::new();
    let args = build_args(&mut alloc, &[a, b]);
    let clvm_result = op_divmod(&mut alloc, args, MAX_COST);
    if b.sign() == Sign::NoSign {
        assert!(clvm_result.is_err(), "divmod({a}, 0): CLVM should fail");
        return;
    }
    let Reduction(_cost, result) = clvm_result.expect("divmod: CLVM failed unexpectedly");
    let (expected_q, expected_r) = a.div_mod_floor(b);
    let SExp::Pair(left, right) = alloc.sexp(result) else {
        panic!("divmod({a}, {b}): result is not a pair");
    };
    assert_eq!(
        alloc.number(left),
        expected_q,
        "divmod({a}, {b}): quotient mismatch"
    );
    assert_eq!(
        alloc.number(right),
        expected_r,
        "divmod({a}, {b}): remainder mismatch"
    );
}

fuzz_target!(|input: (Vec<u8>, Vec<u8>)| {
    let a = Number::from_signed_bytes_be(&input.0);
    let b = Number::from_signed_bytes_be(&input.1);

    check_binary_op(op_div, |a, b| a.div_floor(b), &a, &b, "div");
    check_binary_op(op_mod, |a, b| a.mod_floor(b), &a, &b, "mod");
    check_divmod(&a, &b);
});
