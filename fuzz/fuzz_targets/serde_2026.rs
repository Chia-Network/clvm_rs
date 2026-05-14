#![no_main]

use clvm_fuzzing::ArbitraryClvmTree;
use clvmr::serde_2026::{deserialize_2026, deserialize_2026_body_from_stream, serialize_2026};
use clvmr::{Allocator, allocator::NodePtr};
use libfuzzer_sys::{Corpus, fuzz_target};
use std::io::Cursor;

const FUZZ_MAX_ATOM_LEN: usize = 1 << 20;

#[derive(arbitrary::Arbitrary, Debug)]
enum FuzzInput {
    Bytes(Vec<u8>),
    Tree(Box<ArbitraryClvmTree<10_000, true>>),
}

fn node_eq(allocator: &Allocator, s1: NodePtr, s2: NodePtr) -> bool {
    use clvmr::allocator::SExp;
    let mut stack = vec![(s1, s2)];
    while let Some((l, r)) = stack.pop() {
        match (allocator.sexp(l), allocator.sexp(r)) {
            (SExp::Pair(ll, lr), SExp::Pair(rl, rr)) => {
                stack.push((lr, rr));
                stack.push((ll, rl));
            }
            (SExp::Atom, SExp::Atom) => {
                if !allocator.atom_eq(l, r) {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

fn roundtrip_check(label: &str, a: &mut Allocator, original: NodePtr, blob: &[u8]) {
    let checkpoint = a.checkpoint();
    let decoded = deserialize_2026(a, blob, FUZZ_MAX_ATOM_LEN, false)
        .unwrap_or_else(|e| panic!("{label}: deserialize failed: {e:?}"));
    assert!(
        node_eq(a, original, decoded),
        "{label}: tree mismatch"
    );
    a.restore_checkpoint(&checkpoint);
}

fn check_tree(a: &mut Allocator, node: NodePtr) {
    for (label, level) in serialization_strategies() {
        let blob = serialize_2026(a, node, level).unwrap_or_else(|_| panic!("{label} failed"));
        roundtrip_check(label, a, node, &blob);
    }
}

fn serialization_strategies() -> impl Iterator<Item = (&'static str, u32)> {
    std::iter::once(("fast", 0))
}

fuzz_target!(|input: FuzzInput| -> Corpus {
    match input {
        FuzzInput::Bytes(data) => {
            let mut a = Allocator::new();
            let Ok(node) = deserialize_2026_body_from_stream(
                &mut a,
                &mut Cursor::new(&data),
                FUZZ_MAX_ATOM_LEN,
                false,
            ) else {
                return Corpus::Reject;
            };
            check_tree(&mut a, node);
        }
        FuzzInput::Tree(mut program) => {
            check_tree(&mut program.allocator, program.tree);

            let checkpoint = program.allocator.checkpoint();
            let blob = serialize_2026(&program.allocator, program.tree, 0).expect("Fast failed");
            let decoded = deserialize_2026(&mut program.allocator, &blob, FUZZ_MAX_ATOM_LEN, false)
                .expect("deserialize fast failed");
            assert!(
                node_eq(&program.allocator, program.tree, decoded),
                "FuzzInput::Tree roundtrip mismatch"
            );
            program.allocator.restore_checkpoint(&checkpoint);
        }
    }

    Corpus::Keep
});
