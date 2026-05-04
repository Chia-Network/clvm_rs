#![no_main]

use clvm_fuzzing::ArbitraryClvmTree;
use clvmr::serde::node_to_bytes;
use clvmr::serde_2026::{deserialize_2026_body, serialize_2026_level};
use clvmr::{Allocator, allocator::NodePtr};
use libfuzzer_sys::{Corpus, fuzz_target};

const FUZZ_MAX_ATOM_LEN: usize = 1 << 20;

#[derive(arbitrary::Arbitrary, Debug)]
enum FuzzInput {
    Bytes(Vec<u8>),
    Tree(Box<ArbitraryClvmTree<10_000, true>>),
}

fn canonical(a: &Allocator, node: NodePtr) -> Vec<u8> {
    node_to_bytes(a, node).expect("node_to_bytes failed")
}

fn roundtrip_check(label: &str, a: &Allocator, original: NodePtr, blob: &[u8]) {
    let mut a2 = Allocator::new();
    let decoded = deserialize_2026_body(&mut a2, blob, FUZZ_MAX_ATOM_LEN, false)
        .unwrap_or_else(|e| panic!("{label}: deserialize failed: {e:?}"));
    assert_eq!(
        canonical(a, original),
        canonical(&a2, decoded),
        "{label}: tree mismatch"
    );
}

fn check_tree(a: &Allocator, node: NodePtr) {
    for (label, level) in serialization_strategies() {
        let blob =
            serialize_2026_level(a, node, level).unwrap_or_else(|_| panic!("{label} failed"));
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
            let Ok(node) = deserialize_2026_body(&mut a, &data, FUZZ_MAX_ATOM_LEN, false) else {
                return Corpus::Reject;
            };
            check_tree(&a, node);
        }
        FuzzInput::Tree(program) => {
            check_tree(&program.allocator, program.tree);

            let mut a2 = Allocator::new();
            let blob =
                serialize_2026_level(&program.allocator, program.tree, 0).expect("Fast failed");
            let decoded = deserialize_2026_body(&mut a2, &blob, FUZZ_MAX_ATOM_LEN, false)
                .expect("deserialize fast failed");
            assert_eq!(
                canonical(&program.allocator, program.tree),
                canonical(&a2, decoded)
            );
        }
    }

    Corpus::Keep
});
