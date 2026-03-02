#![no_main]

use clvm_fuzzing::build_args;
use clvmr::allocator::{Allocator, NodePtr};
use clvmr::cost::Cost;
use clvmr::more_ops::{
    op_add, op_ash, op_gr, op_logand, op_logior, op_lognot, op_logxor, op_lsh, op_multiply,
    op_subtract,
};
use clvmr::number::Number;
use clvmr::reduction::Response;
use libfuzzer_sys::fuzz_target;
use num_bigint::BigUint;
use num_traits::ToPrimitive;

const MAX_COST: Cost = 6_000_000_000;

type Opf = fn(&mut Allocator, NodePtr, Cost) -> Response;

fn check_op(
    op: Opf,
    reference: impl Fn(&Number, &Number) -> Number,
    a: &Number,
    b: &Number,
    name: &str,
) {
    let mut alloc = Allocator::new();
    let args = build_args(&mut alloc, &[a, b]);
    let reduction = op(&mut alloc, args, MAX_COST).expect(name);
    assert_eq!(
        alloc.number(reduction.1),
        reference(a, b),
        "{name}({a}, {b}): result mismatch"
    );
}

fuzz_target!(|input: (Vec<u8>, Vec<u8>)| {
    let a = Number::from_signed_bytes_be(&input.0);
    let b = Number::from_signed_bytes_be(&input.1);

    // Binary ops
    check_op(op_add, |a, b| a + b, &a, &b, "add");
    check_op(op_subtract, |a, b| a - b, &a, &b, "subtract");
    check_op(op_multiply, |a, b| a * b, &a, &b, "multiply");
    check_op(op_logand, |a, b| a & b, &a, &b, "logand");
    check_op(op_logior, |a, b| a | b, &a, &b, "logior");
    check_op(op_logxor, |a, b| a ^ b, &a, &b, "logxor");
    check_op(
        op_gr,
        |a, b| if a > b { 1.into() } else { 0.into() },
        &a,
        &b,
        "gr",
    );

    // Unary
    let mut alloc = Allocator::new();
    let args = build_args(&mut alloc, &[&a]);
    let reduction = op_lognot(&mut alloc, args, MAX_COST).expect("lognot");
    assert_eq!(
        alloc.number(reduction.1),
        !&a,
        "lognot({a}): result mismatch"
    );

    // Shift ops: shift amount must fit in i32 and be in -65535..=65535
    let valid_shift = b.to_i32().filter(|s| (-65535..=65535).contains(s));

    // Arithmetic shift (signed)
    let mut alloc = Allocator::new();
    let args = build_args(&mut alloc, &[&a, &b]);
    let clvm_result = op_ash(&mut alloc, args, MAX_COST);
    if let Some(shift) = valid_shift {
        let reduction = clvm_result.expect("ash");
        let expected: Number = if shift > 0 { &a << shift } else { &a >> -shift };
        assert_eq!(
            alloc.number(reduction.1),
            expected,
            "ash({a}, {b}): result mismatch"
        );
    } else {
        assert!(
            clvm_result.is_err(),
            "ash({a}, {b}): CLVM should reject invalid shift"
        );
    }

    // Logical shift (unsigned)
    let mut alloc = Allocator::new();
    let args = build_args(&mut alloc, &[&a, &b]);
    let clvm_result = op_lsh(&mut alloc, args, MAX_COST);
    if let Some(shift) = valid_shift {
        let reduction = clvm_result.expect("lsh");
        // op_lsh interprets the first argument's atom bytes as unsigned
        let unsigned: Number = BigUint::from_bytes_be(&a.to_signed_bytes_be()).into();
        let expected: Number = if shift > 0 {
            &unsigned << shift
        } else {
            &unsigned >> -shift
        };
        assert_eq!(
            alloc.number(reduction.1),
            expected,
            "lsh({a}, {b}): result mismatch"
        );
    } else {
        assert!(
            clvm_result.is_err(),
            "lsh({a}, {b}): CLVM should reject invalid shift"
        );
    }
});
