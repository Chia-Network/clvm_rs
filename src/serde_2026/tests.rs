//! Tests for the 2026 serialization format.

use super::{
    SERDE_2026_MAGIC_PREFIX, deserialize_2026, deserialize_2026_body_from_stream, serialize_2026,
    serialized_length_serde_2026,
};
use crate::allocator::Allocator;
use crate::serde::{node_from_bytes_backrefs, node_to_bytes};
use hex::FromHex;
use rstest::rstest;
use std::io::Cursor;

/// Sane non-consensus default for tests; the public API has no opinion.
const TEST_MAX_ATOM_LEN: usize = 1 << 20;

// ---------------------------------------------------------------------------
// Double-round-trip, all strategies, full corpus
// ---------------------------------------------------------------------------

/// For each legacy-hex tree: serialize with each strategy, then deserialize,
/// re-serialize, and assert identical bytes (idempotency) plus tree equivalence
/// to the original.
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

    let blobs: Vec<(&str, u32, Vec<u8>)> =
        vec![("fast", 0, serialize_2026(&allocator, node, 0).unwrap())];

    for (label, level, blob) in &blobs {
        // First trip: tree equivalence
        let mut a2 = Allocator::new();
        let n2 = deserialize_2026(&mut a2, &blob, TEST_MAX_ATOM_LEN, false).unwrap();
        assert_eq!(
            node_to_bytes(&a2, n2).unwrap(),
            canonical,
            "{label}: tree mismatch for {hex}"
        );

        // Second trip: serialization idempotency (same compression level)
        let blob2 = serialize_2026(&a2, n2, *level).unwrap();
        assert_eq!(
            blob, &blob2,
            "{label}: double round-trip mismatch for {hex}"
        );
    }
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
        deserialize_2026_body_from_stream(
            &mut allocator,
            &mut Cursor::new(data),
            TEST_MAX_ATOM_LEN,
            false
        )
        .is_err(),
        "should reject: {_desc}"
    );
}

#[test]
fn test_strict_rejects_overlong_varints() {
    let mut allocator = Allocator::new();

    // group_count=1 encoded as two bytes, then a valid single atom payload.
    let overlong_group_count = [0x80, 0x01, 0x01, b'A', 0x01, 0x02];
    assert!(
        deserialize_2026_body_from_stream(
            &mut allocator,
            &mut Cursor::new(&overlong_group_count),
            TEST_MAX_ATOM_LEN,
            true
        )
        .is_err()
    );

    let decoded = deserialize_2026_body_from_stream(
        &mut allocator,
        &mut Cursor::new(&overlong_group_count),
        TEST_MAX_ATOM_LEN,
        false,
    )
    .unwrap();
    assert_eq!(allocator.atom(decoded).as_ref(), b"A");
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
    let bytes = serialize_2026(&allocator, node, 0).unwrap();
    assert!(bytes.starts_with(&SERDE_2026_MAGIC_PREFIX));
}

// Auto-detection (sniff the magic prefix and dispatch) is a Python-only
// convenience now, exposed by `clvm_rs.serde.deserialize(blob, "auto")`.
// See `wheel/python/tests/test_serialize.py` for coverage.

#[test]
fn test_backrefs_decoder_rejects_serde_2026() {
    let mut allocator = Allocator::new();
    let node = allocator.new_atom(b"hello").unwrap();
    let prefixed = serialize_2026(&allocator, node, 0).unwrap();
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
    let bytes = serialize_2026(&allocator, node, 0).unwrap();
    assert_eq!(
        serialized_length_serde_2026(&bytes, TEST_MAX_ATOM_LEN, false).unwrap(),
        bytes.len() as u64
    );

    // pair
    let left = allocator.new_atom(b"left").unwrap();
    let right = allocator.new_atom(b"right").unwrap();
    let pair = allocator.new_pair(left, right).unwrap();
    let bytes = serialize_2026(&allocator, pair, 0).unwrap();
    assert_eq!(
        serialized_length_serde_2026(&bytes, TEST_MAX_ATOM_LEN, false).unwrap(),
        bytes.len() as u64
    );

    // complex tree with shared subtrees
    let a = allocator.new_atom(b"shared").unwrap();
    let p1 = allocator.new_pair(a, a).unwrap();
    let b = allocator.new_atom(b"other").unwrap();
    let p2 = allocator.new_pair(p1, b).unwrap();
    let root = allocator.new_pair(p2, p1).unwrap();
    let bytes = serialize_2026(&allocator, root, 0).unwrap();
    assert_eq!(
        serialized_length_serde_2026(&bytes, TEST_MAX_ATOM_LEN, false).unwrap(),
        bytes.len() as u64
    );

    // with trailing data — length should exclude it
    let mut padded = bytes.clone();
    padded.extend_from_slice(b"trailing garbage");
    assert_eq!(
        serialized_length_serde_2026(&padded, TEST_MAX_ATOM_LEN, false).unwrap(),
        bytes.len() as u64
    );

    // rejects non-prefixed / empty
    assert!(serialized_length_serde_2026(b"\x80", TEST_MAX_ATOM_LEN, false).is_err());
    assert!(serialized_length_serde_2026(b"", TEST_MAX_ATOM_LEN, false).is_err());
}

/// `serialized_length_serde_2026` must reject every header-time condition
/// that `deserialize_2026_body` rejects, so callers can use the length helper to
/// gate before deserializing without observing Ok-then-Err mismatches.
#[test]
fn test_serialized_length_rejects_what_deserialize_rejects() {
    use super::varint::encode_varint;

    let mk = |group_count: i64, instruction_count: i64, atom_table: &[u8]| {
        let mut blob = Vec::new();
        blob.extend_from_slice(&SERDE_2026_MAGIC_PREFIX);
        blob.extend_from_slice(&encode_varint(group_count));
        blob.extend_from_slice(atom_table);
        blob.extend_from_slice(&encode_varint(instruction_count));
        blob
    };

    // Cases that deserialize_2026_body rejects at header time. For each one,
    // serialized_length_serde_2026 must also reject.
    let cases: &[(&str, Vec<u8>)] = &[
        // instruction_count = 0 with non-empty atom table
        (
            "instruction_count == 0",
            mk(1, 0, &[0x01, b'A']), // one group, length=1, atom='A'
        ),
        // group with length = 0
        ("group length == 0", mk(1, 1, &[0x00])),
        // multi-atom group with count = 0
        (
            "multi-atom group count == 0",
            mk(1, 1, &[0x7f, 0x00, b'A']), // length=-1 (multi-atom), count=0
        ),
    ];

    let mut a = Allocator::new();
    for (label, blob) in cases {
        // Use the prefix-aware deserializer so the asymmetry under test
        // (header-time rejections) is what fails, not the magic-prefix check.
        assert!(
            deserialize_2026(&mut a, blob, TEST_MAX_ATOM_LEN, false).is_err(),
            "{label}: deserialize must reject"
        );
        assert!(
            serialized_length_serde_2026(blob, TEST_MAX_ATOM_LEN, false).is_err(),
            "{label}: serialized_length must reject (mirrors deserialize)"
        );
    }
}

// ---------------------------------------------------------------------------
// Regression test for the unbounded-capacity OOM. A tiny blob
// (under 16 bytes) declares an `instruction_count` near the max representable
// varint (~2^54). Pre-fix, the deserializer pre-allocated `instruction_count
// / 3` `NodePtr`s — a request of about 24 PB — and the process aborted with
// "memory allocation of N bytes failed". Post-fix, the deserializer starts
// with `Vec::new()` and is bounded by the input slice (or caller-supplied
// `Read::take`), so the loop runs out of bytes long before it can drive the
// vector to a pathological size and we return `Err` cleanly.
#[test]
fn deserializer_rejects_unbounded_instruction_count() {
    use super::varint::encode_varint;

    let mut blob = Vec::new();
    blob.extend_from_slice(&encode_varint(0)); // group_count = 0
    blob.extend_from_slice(&encode_varint(1_i64 << 54)); // instruction_count
    assert!(
        blob.len() < 16,
        "PoC blob stays tiny ({} bytes)",
        blob.len()
    );

    let mut a = Allocator::new();
    let result = deserialize_2026_body_from_stream(
        &mut a,
        &mut Cursor::new(&blob),
        TEST_MAX_ATOM_LEN,
        false,
    );
    assert!(
        result.is_err(),
        "instruction_count must be rejected before pre-allocation"
    );
}

#[test]
fn deserializer_rejects_unbounded_group_count() {
    use super::varint::encode_varint;

    let mut blob = Vec::new();
    blob.extend_from_slice(&encode_varint(1_i64 << 54)); // group_count

    let mut a = Allocator::new();
    let result = deserialize_2026_body_from_stream(
        &mut a,
        &mut Cursor::new(&blob),
        TEST_MAX_ATOM_LEN,
        false,
    );
    assert!(
        result.is_err(),
        "group_count must be rejected before pre-allocation"
    );
}

#[test]
fn deserializer_rejects_unbounded_per_group_count() {
    use super::varint::encode_varint;

    let mut blob = Vec::new();
    blob.extend_from_slice(&encode_varint(1)); // group_count = 1
    blob.extend_from_slice(&encode_varint(-3)); // multi-atom group, len=3
    blob.extend_from_slice(&encode_varint(1_i64 << 54)); // count

    let mut a = Allocator::new();
    let result = deserialize_2026_body_from_stream(
        &mut a,
        &mut Cursor::new(&blob),
        TEST_MAX_ATOM_LEN,
        false,
    );
    assert!(
        result.is_err(),
        "per-group count must be rejected before pre-allocation"
    );
}

// ---------------------------------------------------------------------------
// write_atom_table
// ---------------------------------------------------------------------------

/// Verify the wire-level structure of `write_atom_table`: atoms of the same
/// length are emitted as a single repeated-length group (negative length
/// varint + count), and atoms of different lengths split into separate groups.
/// The exact ordering of atoms inside a length group depends on the frequency
/// sort; this test only inspects structure, not order.
#[test]
fn test_write_atom_table_groups_by_length() {
    use super::ser::{SerializerState, write_atom_table};
    use super::varint::read_varint;
    use std::io::{Cursor, Read};

    // Tree with three 3-byte atoms and one 5-byte atom — every atom is forced
    // into the table (no nil, no duplicates).
    let mut a = Allocator::new();
    let foo = a.new_atom(b"foo").unwrap();
    let bar = a.new_atom(b"bar").unwrap();
    let baz = a.new_atom(b"baz").unwrap();
    let hello = a.new_atom(b"hello").unwrap();
    let p = a.new_pair(foo, bar).unwrap();
    let q = a.new_pair(baz, hello).unwrap();
    let root = a.new_pair(p, q).unwrap();

    let state = SerializerState::new(&a, root).unwrap();
    let mut buf = Vec::new();
    write_atom_table(&mut buf, &state.tree, &state.sorted_no_nil).unwrap();

    let mut cursor = Cursor::new(&buf[..]);
    let group_count = read_varint(&mut cursor, false).unwrap();
    assert_eq!(group_count, 2, "expected 2 length-groups (3, 5)");

    let mut total_atoms = 0usize;
    let mut total_bytes = 0usize;
    let mut saw_repeated_3 = false;
    let mut saw_singleton_5 = false;
    for _ in 0..group_count {
        let length_val = read_varint(&mut cursor, false).unwrap();
        if length_val < 0 {
            // multi-atom group: -length, count, then count*length raw bytes
            let len = (-length_val) as usize;
            let count = read_varint(&mut cursor, false).unwrap() as usize;
            assert!(len > 0 && count > 1);
            let mut bytes = vec![0u8; len * count];
            cursor.read_exact(&mut bytes).unwrap();
            total_atoms += count;
            total_bytes += bytes.len();
            if len == 3 && count == 3 {
                saw_repeated_3 = true;
            }
        } else {
            // singleton group: positive length, then `length` raw bytes
            let len = length_val as usize;
            let mut bytes = vec![0u8; len];
            cursor.read_exact(&mut bytes).unwrap();
            total_atoms += 1;
            total_bytes += bytes.len();
            if len == 5 {
                saw_singleton_5 = true;
            }
        }
    }

    assert!(
        saw_repeated_3,
        "expected the three 3-byte atoms to share a group"
    );
    assert!(
        saw_singleton_5,
        "expected the 5-byte atom as a singleton group"
    );
    assert_eq!(total_atoms, 4);
    assert_eq!(total_bytes, 3 * 3 + 5);
    assert_eq!(
        cursor.position() as usize,
        buf.len(),
        "all bytes of the atom table should be consumed"
    );
}
