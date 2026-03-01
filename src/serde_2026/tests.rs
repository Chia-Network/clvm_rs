//! Tests for the 2026 serialization format.

use super::{deserialize_2026, serialize_2026};
use crate::allocator::{Allocator, SExp};
use crate::serde::{node_from_bytes_backrefs, node_to_bytes};
use hex::FromHex;
use rstest::rstest;

#[test]
fn test_roundtrip_simple_atom() {
    let mut allocator = Allocator::new();
    let node = allocator.new_atom(b"hello").unwrap();

    let serialized = serialize_2026(&allocator, node).unwrap();
    let mut new_allocator = Allocator::new();
    let deserialized = deserialize_2026(&mut new_allocator, &serialized, None).unwrap();

    let original_atom = allocator.atom(node);
    let deserialized_atom = new_allocator.atom(deserialized);
    assert_eq!(original_atom.as_ref(), deserialized_atom.as_ref());
}

#[test]
fn test_roundtrip_simple_pair() {
    let mut allocator = Allocator::new();
    let left = allocator.new_atom(b"left").unwrap();
    let right = allocator.new_atom(b"right").unwrap();
    let pair = allocator.new_pair(left, right).unwrap();

    let serialized = serialize_2026(&allocator, pair).unwrap();
    let mut new_allocator = Allocator::new();
    let deserialized = deserialize_2026(&mut new_allocator, &serialized, None).unwrap();

    // Check structure
    match new_allocator.sexp(deserialized) {
        SExp::Pair(l, r) => {
            let left_atom = new_allocator.atom(l);
            let right_atom = new_allocator.atom(r);
            assert_eq!(left_atom.as_ref(), b"left");
            assert_eq!(right_atom.as_ref(), b"right");
        }
        _ => panic!("Expected pair"),
    }
}

#[test]
fn test_empty_atom() {
    let mut allocator = Allocator::new();
    let node = allocator.new_atom(b"").unwrap();

    let serialized = serialize_2026(&allocator, node).unwrap();
    let mut new_allocator = Allocator::new();
    let deserialized = deserialize_2026(&mut new_allocator, &serialized, None).unwrap();

    assert_eq!(
        allocator.atom(node).as_ref(),
        new_allocator.atom(deserialized).as_ref()
    );
}

#[test]
fn test_multiple_atoms_same_length() {
    let mut allocator = Allocator::new();
    let a1 = allocator.new_atom(b"AAA").unwrap();
    let a2 = allocator.new_atom(b"BBB").unwrap();
    let a3 = allocator.new_atom(b"CCC").unwrap();

    // Create: (AAA . (BBB . CCC))
    let p1 = allocator.new_pair(a2, a3).unwrap();
    let root = allocator.new_pair(a1, p1).unwrap();

    let serialized = serialize_2026(&allocator, root).unwrap();
    let mut new_allocator = Allocator::new();
    let deserialized = deserialize_2026(&mut new_allocator, &serialized, None).unwrap();

    // Verify structure
    match new_allocator.sexp(deserialized) {
        SExp::Pair(left, right) => {
            assert_eq!(new_allocator.atom(left).as_ref(), b"AAA");
            match new_allocator.sexp(right) {
                SExp::Pair(left2, right2) => {
                    assert_eq!(new_allocator.atom(left2).as_ref(), b"BBB");
                    assert_eq!(new_allocator.atom(right2).as_ref(), b"CCC");
                }
                _ => panic!("Expected pair"),
            }
        }
        _ => panic!("Expected pair"),
    }
}

#[test]
fn test_deduplication() {
    let mut allocator = Allocator::new();
    let a = allocator.new_atom(b"AAA").unwrap();
    let b = allocator.new_atom(b"BBB").unwrap();
    let c = allocator.new_atom(b"CCC").unwrap();

    // Create: ((AAA . BBB) . (CCC . AAA))
    // AAA appears twice and should be deduplicated
    let p1 = allocator.new_pair(a, b).unwrap();
    let p2 = allocator.new_pair(c, a).unwrap();
    let root = allocator.new_pair(p1, p2).unwrap();

    let serialized = serialize_2026(&allocator, root).unwrap();

    // The serialization should be smaller due to deduplication
    // We expect: 3 unique atoms (AAA, BBB, CCC) + structure
    println!("Deduplication test: {} bytes", serialized.len());

    let mut new_allocator = Allocator::new();
    let deserialized = deserialize_2026(&mut new_allocator, &serialized, None).unwrap();

    // Verify the structure is correct
    match new_allocator.sexp(deserialized) {
        SExp::Pair(left, right) => {
            // left should be (AAA . BBB)
            match new_allocator.sexp(left) {
                SExp::Pair(l1, r1) => {
                    assert_eq!(new_allocator.atom(l1).as_ref(), b"AAA");
                    assert_eq!(new_allocator.atom(r1).as_ref(), b"BBB");
                }
                _ => panic!("Expected pair for left"),
            }
            // right should be (CCC . AAA)
            match new_allocator.sexp(right) {
                SExp::Pair(l2, r2) => {
                    assert_eq!(new_allocator.atom(l2).as_ref(), b"CCC");
                    assert_eq!(new_allocator.atom(r2).as_ref(), b"AAA");
                }
                _ => panic!("Expected pair for right"),
            }
        }
        _ => panic!("Expected pair at root"),
    }
}

#[test]
fn test_list_structure() {
    let mut allocator = Allocator::new();
    let nil = allocator.nil();

    // Create list: (1 2 3 4 5)
    let mut result = nil;
    for i in (1..=5).rev() {
        let atom = allocator.new_atom(&[i]).unwrap();
        result = allocator.new_pair(atom, result).unwrap();
    }

    let serialized = serialize_2026(&allocator, result).unwrap();
    println!("List (1 2 3 4 5): {} bytes", serialized.len());

    let mut new_allocator = Allocator::new();
    let deserialized = deserialize_2026(&mut new_allocator, &serialized, None).unwrap();

    // Verify list elements
    let mut current = deserialized;
    for expected in 1..=5 {
        match new_allocator.sexp(current) {
            SExp::Pair(left, right) => {
                assert_eq!(new_allocator.atom(left).as_ref(), &[expected]);
                current = right;
            }
            _ => panic!("Expected pair"),
        }
    }
    // Should end with nil
    assert_eq!(new_allocator.atom(current).as_ref(), b"");
}

#[test]
fn test_deeply_nested() {
    let mut allocator = Allocator::new();
    let leaf = allocator.new_atom(b"leaf").unwrap();

    // Create deeply nested structure: (leaf . (leaf . (leaf . ... )))
    let mut result = leaf;
    for _ in 0..100 {
        result = allocator.new_pair(leaf, result).unwrap();
    }

    let serialized = serialize_2026(&allocator, result).unwrap();
    println!("Deeply nested (depth 100): {} bytes", serialized.len());

    let mut new_allocator = Allocator::new();
    let deserialized = deserialize_2026(&mut new_allocator, &serialized, None).unwrap();

    // Verify depth
    let mut current = deserialized;
    let mut depth = 0;
    loop {
        match new_allocator.sexp(current) {
            SExp::Pair(left, right) => {
                assert_eq!(new_allocator.atom(left).as_ref(), b"leaf");
                depth += 1;
                current = right;
            }
            SExp::Atom => {
                assert_eq!(new_allocator.atom(current).as_ref(), b"leaf");
                break;
            }
        }
    }
    assert_eq!(depth, 100);
}

#[test]
fn test_various_atom_sizes() {
    let mut allocator = Allocator::new();

    // Create atoms of various sizes
    let empty = allocator.new_atom(b"").unwrap();
    let small = allocator.new_atom(b"x").unwrap();
    let medium = allocator.new_atom(&[42; 100]).unwrap();
    let large = allocator.new_atom(&[7; 1000]).unwrap();

    // Build structure
    let p1 = allocator.new_pair(empty, small).unwrap();
    let p2 = allocator.new_pair(medium, large).unwrap();
    let root = allocator.new_pair(p1, p2).unwrap();

    let serialized = serialize_2026(&allocator, root).unwrap();
    println!("Various sizes: {} bytes", serialized.len());

    let mut new_allocator = Allocator::new();
    let deserialized = deserialize_2026(&mut new_allocator, &serialized, None).unwrap();

    // Verify structure
    match new_allocator.sexp(deserialized) {
        SExp::Pair(left, right) => {
            match new_allocator.sexp(left) {
                SExp::Pair(l1, r1) => {
                    assert_eq!(new_allocator.atom(l1).as_ref(), b"");
                    assert_eq!(new_allocator.atom(r1).as_ref(), b"x");
                }
                _ => panic!("Expected pair"),
            }
            match new_allocator.sexp(right) {
                SExp::Pair(l2, r2) => {
                    assert_eq!(new_allocator.atom(l2).as_ref().len(), 100);
                    assert_eq!(new_allocator.atom(r2).as_ref().len(), 1000);
                }
                _ => panic!("Expected pair"),
            }
        }
        _ => panic!("Expected pair"),
    }
}

/// Round-trip through 2026 format: deserialize from legacy hex -> serialize_2026 -> deserialize_2026 -> verify via node_to_bytes.
/// Mirrors the structures tested in serde/test.rs test_round_trip.
#[rstest]
#[case("01")] // 1
#[case("ff83666f6f83626172")] // (foo . bar)
#[case("ff83666f6fff8362617280")] // (foo bar)
#[case("ffff0102ff0304")] // ((1 . 2) . (3 . 4))
#[case("ff01ff02ff03ff04ff05ff0680")] // (1 2 3 4 5 6)
#[case("ff83666f6ffe02")] // (foo . foo)
#[case("ff846c6f6e67ff86737472696e67ff826f66fffe0bff8474657874fffe1780")] // (long string of text)
#[case("ff83666f6ffffe01fffe01fffe01fffe01fffe01fffe0180")] // (foo (foo) ((foo) foo) ...) - parse stack backrefs
fn test_round_trip_from_legacy(#[case] hex: &str) {
    let bytes = Vec::from_hex(hex).unwrap();
    let mut allocator = Allocator::new();
    let node = node_from_bytes_backrefs(&mut allocator, &bytes).unwrap();
    let canonical = node_to_bytes(&allocator, node).unwrap();

    let serialized = serialize_2026(&allocator, node).unwrap();
    let mut new_allocator = Allocator::new();
    let deserialized = deserialize_2026(&mut new_allocator, &serialized, None).unwrap();
    let round_trip_canonical = node_to_bytes(&new_allocator, deserialized).unwrap();

    assert_eq!(canonical, round_trip_canonical, "Round-trip mismatch for hex: {}", hex);
}

#[test]
fn test_nil_roundtrip() {
    let allocator = Allocator::new();
    let nil = allocator.nil();

    let serialized = serialize_2026(&allocator, nil).unwrap();
    let mut new_allocator = Allocator::new();
    let deserialized = deserialize_2026(&mut new_allocator, &serialized, None).unwrap();

    assert_eq!(new_allocator.atom(deserialized).as_ref(), b"");
}

#[test]
fn test_round_trip_double() {
    // Serialize -> deserialize -> serialize again -> deserialize again; both final forms must match.
    let mut allocator = Allocator::new();
    let node = node_from_bytes_backrefs(
        &mut allocator,
        &Vec::from_hex("ff01ff02ff0300").unwrap(),
    )
    .unwrap();

    let serialized1 = serialize_2026(&allocator, node).unwrap();
    let mut allocator2 = Allocator::new();
    let node2 = deserialize_2026(&mut allocator2, &serialized1, None).unwrap();
    let serialized2 = serialize_2026(&allocator2, node2).unwrap();
    assert_eq!(serialized1, serialized2, "Double round-trip should produce identical bytes");
}

#[test]
fn test_deserialize_rejects_malformed_counts() {
    let mut allocator = Allocator::new();

    // Negative atom_lengths_count (0x7f = -1 in single-byte varint) would have caused huge loop
    assert!(deserialize_2026(&mut allocator, &[0x7f], None).is_err());

    // 0xFF is invalid varint prefix
    assert!(deserialize_2026(&mut allocator, &[0xff], None).is_err());
}

/// Round-trip through 2026 format for all structures from serde/test_intern.rs.
/// Verifies 2026 format correctly handles the same structures the intern module is tested with.
#[rstest]
#[case("01")] // Simple atom 1
#[case("0a")] // Atom 10
#[case("ff0101")] // (1 . 1)
#[case("ff010a")] // (1 . 10)
#[case("ff01ff0101")] // (1 . (1 . 1))
#[case("ffff2a2a2a")] // ((42 . 42) . 42)
#[case("ff01ff02ff0301")] // (1 . (2 . (3 . 1)))
#[case("ff01ff02ff0300")] // (1 . (2 . (3 . nil)))
#[case("ff01ff02ff0304")] // (1 . (2 . (3 . 4)))
#[case("ff01ff02ff0103")] // (1 . (2 . (1 . 3)))
#[case("ffff0102ff0102")] // ((1 . 2) . (1 . 2))
#[case("ffff0102ffff0102ff0102")] // ((1 . 2) . ((1 . 2) . (1 . 2)))
#[case("ffff0102ffff010200")] // ((1 . 2) . ((1 . 2) . nil))
#[case("ffff010aff010a")] // ((1 . 10) . (1 . 10))
#[case("00")] // nil
#[case("ff0100")] // (1 . nil)
#[case("ff0000")] // (nil . nil)
#[case("ff01ff01ff0100")] // (1 . (1 . (1 . nil)))
#[case("ff01ff01ff0101")] // (1 . (1 . (1 . 1)))
#[case("ffff01ff0203ff01ff0203")] // ((1.(2.3)) . (1.(2.3)))
#[case("ffff0102ffff0102ffff010200")] // ((1.2) . ((1.2) . ((1.2) . nil)))
fn test_round_trip_intern_structures(#[case] hex: &str) {
    let bytes = Vec::from_hex(hex.trim().replace([' ', '\n'], "")).unwrap();
    let mut allocator = Allocator::new();
    let node = node_from_bytes_backrefs(&mut allocator, &bytes).unwrap();
    let canonical = node_to_bytes(&allocator, node).unwrap();

    let serialized = serialize_2026(&allocator, node).unwrap();
    let mut new_allocator = Allocator::new();
    let deserialized = deserialize_2026(&mut new_allocator, &serialized, None).unwrap();
    let round_trip_canonical = node_to_bytes(&new_allocator, deserialized).unwrap();

    assert_eq!(canonical, round_trip_canonical, "Round-trip mismatch for hex: {}", hex);
}
