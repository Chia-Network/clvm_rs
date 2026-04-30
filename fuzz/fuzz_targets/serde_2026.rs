#![no_main]

use clvm_fuzzing::ArbitraryClvmTree;
use clvmr::serde::node_to_bytes;
use clvmr::serde_2026::{Compression, DeserializeOptions, deserialize_2026, serialize_2026};
use clvmr::{Allocator, allocator::NodePtr};
use libfuzzer_sys::{Corpus, fuzz_target};

#[derive(arbitrary::Arbitrary, Debug)]
enum FuzzInput {
    Bytes(Vec<u8>),
    Tree(ArbitraryClvmTree<10_000, true>),
}

fn canonical(a: &Allocator, node: NodePtr) -> Vec<u8> {
    node_to_bytes(a, node).expect("node_to_bytes failed")
}

fn roundtrip_check(label: &str, a: &Allocator, original: NodePtr, blob: &[u8]) {
    let options = DeserializeOptions::default();
    let mut a2 = Allocator::new();
    let decoded = deserialize_2026(&mut a2, blob, options)
        .unwrap_or_else(|e| panic!("{label}: deserialize failed: {e:?}"));
    assert_eq!(
        canonical(a, original),
        canonical(&a2, decoded),
        "{label}: tree mismatch"
    );
}

fn check_tree(a: &Allocator, node: NodePtr) {
    let fast = serialize_2026(a, node, Compression::Fast).expect("Fast failed");
    let compact = serialize_2026(a, node, Compression::Compact).expect("Compact failed");

    roundtrip_check("fast", a, node, &fast);
    roundtrip_check("compact", a, node, &compact);

    assert!(
        compact.len() <= fast.len(),
        "compact ({}) > fast ({})",
        compact.len(),
        fast.len()
    );
}

fuzz_target!(|input: FuzzInput| -> Corpus {
    match input {
        FuzzInput::Bytes(data) => {
            let mut a = Allocator::new();
            let options = DeserializeOptions::default();
            let Ok(node) = deserialize_2026(&mut a, &data, options) else {
                return Corpus::Reject;
            };
            check_tree(&a, node);
        }
        FuzzInput::Tree(program) => {
            check_tree(&program.allocator, program.tree);

            let mut a2 = Allocator::new();
            let blob = serialize_2026(&program.allocator, program.tree, Compression::Compact)
                .expect("Compact failed");
            let decoded = deserialize_2026(&mut a2, &blob, DeserializeOptions::default())
                .expect("deserialize compact failed");
            assert_eq!(
                canonical(&program.allocator, program.tree),
                canonical(&a2, decoded)
            );
        }
    }

    Corpus::Keep
});
