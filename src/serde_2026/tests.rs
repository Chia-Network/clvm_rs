//! Tests for the 2026 serialization format.

use super::{
    Compression, DeserializeOptions, SERDE_2026_MAGIC_PREFIX, deserialize_2026,
    node_from_bytes_auto, node_to_bytes_serde_2026, serialize_2026, serialized_length_serde_2026,
};
use crate::allocator::Allocator;
use crate::serde::{node_from_bytes_backrefs, node_to_bytes};
use hex::FromHex;
use rstest::rstest;

// ---------------------------------------------------------------------------
// Double-round-trip, all strategies, full corpus
// ---------------------------------------------------------------------------

/// For each legacy-hex tree: serialize with Fast and Compact, then
/// deserialize each, re-serialize, and assert identical bytes (idempotency)
/// plus tree equivalence to the original.
#[rstest]
#[case("00")] // nil
#[case("80")] // empty atom (canonical nil)
#[case("01")] // 1
#[case("0a")] // 10
#[case("8568656c6c6f")] // 5-byte atom "hello"
#[case("8b68656c6c6f20776f726c64")] // 11-byte atom "hello world"
#[case(
    "b8400102030405060708091011121314151617181920212223242526272829303132333435363738394041424344454647484950515253545556575859606162636465"
)] // 64-byte atom
#[case("ff0100")] // (1 . nil)
#[case("ff0000")] // (nil . nil)
#[case("ff0101")] // (1 . 1)
#[case("ff010a")] // (1 . 10)
#[case("ff83666f6f83626172")] // (foo . bar)
#[case("ff83666f6fff8362617280")] // (foo bar)
#[case("ffff0102ff0304")] // ((1 . 2) . (3 . 4))
#[case("ff01ff02ff03ff04ff05ff0680")] // (1 2 3 4 5 6)
#[case("ff83666f6ffe02")] // (foo . foo)
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
#[case("ff01ff01ff0100")] // (1 . (1 . (1 . nil)))
#[case("ff01ff01ff0101")] // (1 . (1 . (1 . 1)))
#[case("ffff01ff0203ff01ff0203")] // ((1.(2.3)) . (1.(2.3)))
#[case("ffff0102ffff0102ffff010200")] // ((1.2) . ((1.2) . ((1.2) . nil)))
#[case("ff846c6f6e67ff86737472696e67ff826f66fffe0bff8474657874fffe1780")] // (long string of text)
#[case("ff83666f6ffffe01fffe01fffe01fffe01fffe01fffe0180")] // backrefs chain
fn test_round_trip(#[case] hex: &str) {
    let bytes = Vec::from_hex(hex).unwrap();
    let mut allocator = Allocator::new();
    let node = node_from_bytes_backrefs(&mut allocator, &bytes).unwrap();
    let canonical = node_to_bytes(&allocator, node).unwrap();
    let options = DeserializeOptions::default();

    let blobs: Vec<(&str, Compression, Vec<u8>)> = vec![
        (
            "fast",
            Compression::Fast,
            serialize_2026(&allocator, node, Compression::Fast).unwrap(),
        ),
        (
            "compact",
            Compression::Compact,
            serialize_2026(&allocator, node, Compression::Compact).unwrap(),
        ),
    ];

    for (label, compression, blob) in &blobs {
        // First trip: tree equivalence
        let mut a2 = Allocator::new();
        let n2 = deserialize_2026(&mut a2, blob, options).unwrap();
        assert_eq!(
            node_to_bytes(&a2, n2).unwrap(),
            canonical,
            "{label}: tree mismatch for {hex}"
        );

        // Second trip: serialization idempotency (same compression level)
        let blob2 = serialize_2026(&a2, n2, *compression).unwrap();
        assert_eq!(
            blob, &blob2,
            "{label}: double round-trip mismatch for {hex}"
        );
    }

    // Compact must never be larger than Fast
    assert!(
        blobs[1].2.len() <= blobs[0].2.len(),
        "compact ({}) > fast ({}) for {hex}",
        blobs[1].2.len(),
        blobs[0].2.len()
    );
}

// ---------------------------------------------------------------------------
// Malformed input rejection
// ---------------------------------------------------------------------------

#[rstest]
#[case(&[0x7f], "negative atom group count")]
#[case(&[0xff], "0xFF invalid varint prefix")]
#[case(&[0x80], "truncated multibyte varint")]
#[case(&[0x80, 0x80], "truncated 3-byte varint")]
#[case(&[], "empty input")]
#[case(&[0x01, 0x01, 0x41], "valid atom table, truncated before instruction count")]
#[case(&[0x01, 0x01, 0x41, 0x02, 0x02], "instruction count=2 but only 1 instruction follows")]
#[case(&[0x01, 0x01, 0x41, 0x01, 0x70], "instruction refs atom index 110, only 1 exists")]
#[case(&[0x01, 0x01, 0x41, 0x00], "zero instructions with non-empty atom table")]
#[case(&[0x00, 0x00], "zero groups and zero instructions")]
#[case(&[0x02, 0x01, 0x41, 0x01, 0x42], "two groups claimed, only one provided")]
fn test_deserialize_rejects_malformed(#[case] data: &[u8], #[case] _desc: &str) {
    let mut allocator = Allocator::new();
    assert!(
        deserialize_2026(&mut allocator, data, DeserializeOptions::default()).is_err(),
        "should reject: {_desc}"
    );
}

#[test]
fn test_strict_rejects_overlong_varints() {
    let mut allocator = Allocator::new();
    let mut options = DeserializeOptions {
        strict: true,
        ..DeserializeOptions::default()
    };

    // group_count=1 encoded as two bytes, then a valid single atom payload.
    let overlong_group_count = [0x80, 0x01, 0x01, b'A', 0x01, 0x02];
    assert!(deserialize_2026(&mut allocator, &overlong_group_count, options).is_err());

    options.strict = false;
    let decoded = deserialize_2026(&mut allocator, &overlong_group_count, options).unwrap();
    assert_eq!(allocator.atom(decoded).as_ref(), b"A");
}

#[test]
fn test_input_byte_limit_bounds_parser_work() {
    let mut allocator = Allocator::new();
    let options = DeserializeOptions {
        max_input_bytes: 3,
        ..DeserializeOptions::default()
    };

    // This otherwise-valid one-atom blob needs 4 bytes of payload:
    // group_count, length, atom byte, instruction_count.
    let data = [0x01, 0x01, b'A', 0x01, 0x02];
    assert!(deserialize_2026(&mut allocator, &data, options).is_err());
}

// ---------------------------------------------------------------------------
// Magic prefix and auto-detection
// ---------------------------------------------------------------------------

#[test]
fn test_magic_prefix() {
    assert_eq!(
        SERDE_2026_MAGIC_PREFIX,
        [0xfd, 0xff, b'2', b'0', b'2', b'6']
    );

    let mut allocator = Allocator::new();
    let node = allocator.new_atom(b"hello").unwrap();
    let bytes = node_to_bytes_serde_2026(&allocator, node, Compression::default()).unwrap();
    assert!(bytes.starts_with(&SERDE_2026_MAGIC_PREFIX));
}

fn check_auto(node_alloc: &Allocator, n: crate::allocator::NodePtr) {
    let classic = node_to_bytes(node_alloc, n).unwrap();

    let mut a = Allocator::new();
    let decoded = node_from_bytes_auto(&mut a, &classic, DeserializeOptions::default()).unwrap();
    assert_eq!(
        node_to_bytes(&a, decoded).unwrap(),
        classic,
        "classic via auto"
    );

    let backrefs = crate::serde::node_to_bytes_backrefs(node_alloc, n).unwrap();
    let mut a2 = Allocator::new();
    let decoded2 = node_from_bytes_auto(&mut a2, &backrefs, DeserializeOptions::default()).unwrap();
    assert_eq!(
        node_to_bytes(&a2, decoded2).unwrap(),
        classic,
        "backrefs via auto"
    );

    let prefixed = node_to_bytes_serde_2026(node_alloc, n, Compression::default()).unwrap();
    let mut a3 = Allocator::new();
    let decoded3 = node_from_bytes_auto(&mut a3, &prefixed, DeserializeOptions::default()).unwrap();
    assert_eq!(
        node_to_bytes(&a3, decoded3).unwrap(),
        classic,
        "serde_2026 via auto"
    );
}

#[test]
fn test_auto_detect() {
    let mut a = Allocator::new();
    // atom
    let atom = a.new_atom(b"hello world").unwrap();
    check_auto(&a, atom);
    // nil
    check_auto(&a, a.nil());
    // pair
    let x = a.new_atom(b"a").unwrap();
    let y = a.new_atom(b"b").unwrap();
    let pair = a.new_pair(x, y).unwrap();
    check_auto(&a, pair);
    // nested list
    let nil = a.nil();
    let t1 = a.new_pair(y, nil).unwrap();
    let t2 = a.new_pair(x, t1).unwrap();
    check_auto(&a, t2);
    // shared subtree
    let shared = a.new_atom(b"shared").unwrap();
    let shared_pair = a.new_pair(shared, shared).unwrap();
    check_auto(&a, shared_pair);
}

#[test]
fn test_auto_detect_rejects_empty() {
    let mut allocator = Allocator::new();
    assert!(node_from_bytes_auto(&mut allocator, b"", DeserializeOptions::default()).is_err());
}

#[test]
fn test_auto_detect_classic_single_byte() {
    let mut a = Allocator::new();
    let node = node_from_bytes_auto(&mut a, &[0x01], DeserializeOptions::default()).unwrap();
    assert_eq!(a.atom(node).as_ref(), b"\x01");
}

#[test]
fn test_backrefs_decoder_rejects_serde_2026() {
    let mut allocator = Allocator::new();
    let node = allocator.new_atom(b"hello").unwrap();
    let prefixed = node_to_bytes_serde_2026(&allocator, node, Compression::default()).unwrap();
    let mut a2 = Allocator::new();
    assert!(node_from_bytes_backrefs(&mut a2, &prefixed).is_err());
}

// ---------------------------------------------------------------------------
// serialized_length_serde_2026
// ---------------------------------------------------------------------------

#[test]
fn test_serialized_length() {
    let mut allocator = Allocator::new();

    // atom
    let node = allocator.new_atom(b"hello").unwrap();
    let bytes = node_to_bytes_serde_2026(&allocator, node, Compression::default()).unwrap();
    assert_eq!(
        serialized_length_serde_2026(&bytes).unwrap(),
        bytes.len() as u64
    );

    // pair
    let left = allocator.new_atom(b"left").unwrap();
    let right = allocator.new_atom(b"right").unwrap();
    let pair = allocator.new_pair(left, right).unwrap();
    let bytes = node_to_bytes_serde_2026(&allocator, pair, Compression::default()).unwrap();
    assert_eq!(
        serialized_length_serde_2026(&bytes).unwrap(),
        bytes.len() as u64
    );

    // complex tree with shared subtrees
    let a = allocator.new_atom(b"shared").unwrap();
    let p1 = allocator.new_pair(a, a).unwrap();
    let b = allocator.new_atom(b"other").unwrap();
    let p2 = allocator.new_pair(p1, b).unwrap();
    let root = allocator.new_pair(p2, p1).unwrap();
    let bytes = node_to_bytes_serde_2026(&allocator, root, Compression::default()).unwrap();
    assert_eq!(
        serialized_length_serde_2026(&bytes).unwrap(),
        bytes.len() as u64
    );

    // with trailing data — length should exclude it
    let mut padded = bytes.clone();
    padded.extend_from_slice(b"trailing garbage");
    assert_eq!(
        serialized_length_serde_2026(&padded).unwrap(),
        bytes.len() as u64
    );

    // rejects non-prefixed / empty
    assert!(serialized_length_serde_2026(b"\x80").is_err());
    assert!(serialized_length_serde_2026(b"").is_err());
}
