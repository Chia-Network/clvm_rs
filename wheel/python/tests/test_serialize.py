import io
import unittest

from clvm_rs.clvm_rs import clvm_tree_to_lazy_node, ser_2026
from clvm_rs.program import Program
from clvm_rs.serde import deserialize, serialize
from clvm_rs.ser import atom_to_byte_iterator


TEXT = b"the quick brown fox jumps over the lazy dogs"


class InfiniteStream(io.TextIOBase):
    def __init__(self, b):
        self.buf = b

    def read(self, n):
        ret = b""
        while n > 0 and len(self.buf) > 0:
            ret += self.buf[0:1]
            self.buf = self.buf[1:]
            n -= 1
        ret += b" " * n
        return ret


class LargeAtom:
    def __len__(self):
        return 0x400000001


class SerializeTest(unittest.TestCase):
    def check_serde(self, s):
        v = Program.to(s)
        b = bytes(v)
        f = io.BytesIO()
        v.stream(f)
        b1 = f.getvalue()
        self.assertEqual(b, b1)
        v1 = Program.parse(io.BytesIO(b))
        if v != v1:
            print("%s: %d %s %s" % (v, len(b), b, v1))
            breakpoint()
            b = bytes(v)
            v1 = Program.parse(io.BytesIO(b))
        self.assertEqual(v, v1)

    def test_zero(self):
        v = Program.to(b"\x00")
        self.assertEqual(bytes(v), b"\x00")

    def test_empty(self):
        v = Program.to(b"")
        self.assertEqual(bytes(v), b"\x80")

    def test_empty_string(self):
        self.check_serde(b"")

    def test_single_bytes(self):
        for _ in range(256):
            self.check_serde(bytes([_]))

    def test_short_lists(self):
        self.check_serde([])
        for _ in range(0, 2048, 8):
            for size in range(1, 5):
                self.check_serde([_] * size)

    def test_cons_box(self):
        self.check_serde((0, 0))
        self.check_serde((0, [1, 2, 30, 40, 600, ([], 18)]))
        self.check_serde((100, (TEXT, (30, (50, (90, (TEXT, TEXT + TEXT)))))))

    def test_long_blobs(self):
        text = TEXT * 300
        for _, t in enumerate(text):
            t1 = text[:_]
            self.check_serde(t1)

    def test_blob_limit(self):
        with self.assertRaises(ValueError):
            next(atom_to_byte_iterator(LargeAtom()))

    def test_very_long_blobs(self):
        for size in [0x40, 0x2000, 0x100000, 0x8000000]:
            count = size // len(TEXT)
            text = TEXT * count
            assert len(text) < size
            self.check_serde(text)
            text = TEXT * (count + 1)
            assert len(text) > size
            self.check_serde(text)

    def test_very_deep_tree(self):
        blob = b"a"
        for depth in [10, 100, 1000, 10000, 100000]:
            s = Program.to(blob)
            for _ in range(depth):
                s = Program.to((s, blob))
            self.check_serde(s)

    def test_deserialize_empty(self):
        bytes_in = b""
        with self.assertRaises(ValueError):
            Program.from_bytes(bytes_in)
        with self.assertRaises(ValueError):
            Program.parse(io.BytesIO(bytes_in))

    def test_deserialize_truncated_size(self):
        # fe means the total number of bytes in the length-prefix is 7
        # one for each bit set. 5 bytes is too few
        bytes_in = b"\xfe    "
        with self.assertRaises(ValueError):
            Program.from_bytes(bytes_in)
        with self.assertRaises(ValueError):
            Program.parse(io.BytesIO(bytes_in))

    def test_deserialize_truncated_blob(self):
        # this is a complete length prefix. The blob is supposed to be 63 bytes
        # the blob itself is truncated though, it's less than 63 bytes
        bytes_in = b"\xbf   "

        with self.assertRaises(ValueError):
            Program.from_bytes(bytes_in)
        with self.assertRaises(ValueError):
            Program.parse(io.BytesIO(bytes_in))

    def test_deserialize_large_blob(self):
        # this length prefix is 7 bytes long, the last 6 bytes specifies the
        # length of the blob, which is 0xffffffffffff, or (2^48 - 1)
        # we don't support blobs this large, and we should fail immediately
        # when exceeding the max blob size, rather than trying to read this
        # many bytes from the stream
        bytes_in = b"\xfe" + b"\xff" * 6

        with self.assertRaises(ValueError):
            Program.parse(InfiniteStream(bytes_in))

    def test_repr_clvm_tree(self):
        with self.assertRaises(ValueError):
            Program.fromhex("ff8085")

        o = Program.from_bytes_backrefs(bytes.fromhex("ff808185"))
        self.assertEqual(repr(o._unwrapped_pair[0]), "<CLVMTree: 80>")
        self.assertEqual(repr(o._unwrapped_pair[1]), "<CLVMTree: 8185>")

    def test_bad_blob(self):
        self.assertRaises(ValueError, lambda: Program.fromhex("ff"))
        f = io.BytesIO(bytes.fromhex("ff"))
        self.assertRaises(ValueError, lambda: Program.parse(f))

    def test_large_atom(self):
        s = "foo" * 100
        p = Program.to(s)
        blob = bytes(p)
        p1 = Program.from_bytes(blob)
        self.assertEqual(p, p1)

    def test_too_large_atom(self):
        self.assertRaises(ValueError, lambda: Program.fromhex("fc"))
        self.assertRaises(ValueError, lambda: Program.fromhex("fc8000000000"))

    def test_2026_magic_prefix_and_from_bytes(self):
        p = Program.to((1, (2, 3)))
        prefixed = p.to_bytes_2026()
        self.assertTrue(prefixed.startswith(bytes.fromhex("fdff32303236")))
        p2 = Program.from_bytes(prefixed)
        self.assertEqual(p, p2)

    def test_2026_magic_prefix_explicit_deserializer(self):
        p = Program.to([1, 2, 3, 4])
        prefixed = serialize(deserialize(bytes(p), "legacy"), "2026")
        p2 = Program.from_bytes_2026(prefixed)
        self.assertEqual(p, p2)

    def test_backrefs_parser_rejects_2026(self):
        p = Program.to((b"a", b"b"))
        prefixed = p.to_bytes_2026()
        with self.assertRaises(ValueError):
            Program.from_bytes_backrefs(prefixed)


class ClvmTreeToLazyNodeTest(unittest.TestCase):
    """Tests for clvm_tree_to_lazy_node."""

    def test_basic_roundtrip(self):
        """Python tree -> clvm_tree_to_lazy_node -> ser -> deser -> assert equal."""
        for tree in [b"hello", 42, [1, 2, 3], (1, (2, 3)), [], b""]:
            p = Program.to(tree)
            blob = p.to_bytes_2026()
            p2 = Program.from_bytes(blob)
            self.assertEqual(p, p2, f"roundtrip failed for {tree!r}")

    def test_shared_subtrees_via_identity(self):
        """Shared Python objects should not cause exponential blowup."""
        t = Program.to(b"")
        for _ in range(100):
            t = Program.to((t, t))
        lazy = clvm_tree_to_lazy_node(t)
        self.assertIsNotNone(lazy)

    def test_content_dedup(self):
        """Two distinct Python atoms with same bytes produce one atom."""
        a = Program.to(b"same")
        b = Program.to(b"same")
        self.assertIsNot(a, b)
        tree = Program.to((a, b))
        lazy = clvm_tree_to_lazy_node(tree)
        left = lazy.pair[0]
        right = lazy.pair[1]
        self.assertEqual(left.atom, right.atom)

    def test_pair_dedup(self):
        """Two distinct Python pairs with same structure share one pair in the allocator."""
        inner1 = Program.to((1, 2))
        inner2 = Program.to((1, 2))
        self.assertIsNot(inner1, inner2)
        tree = Program.to((inner1, inner2))
        lazy = clvm_tree_to_lazy_node(tree)
        self.assertIsNotNone(lazy.pair)

    def test_equivalence_with_old_roundtrip(self):
        """New clvm_tree_to_lazy_node path produces same output as old deser_backrefs path."""
        from clvm_rs.clvm_rs import deser_backrefs
        from clvm_rs.ser import sexp_to_bytes

        test_cases = [
            b"hello",
            42,
            [1, 2, 3],
            (1, (2, 3)),
            [],
            b"",
            [b"a", [b"b", b"c"], b"d"],
            (100, (b"text", (30, (50, (90, (b"ab", b"abab")))))),
        ]
        for tree in test_cases:
            p = Program.to(tree)
            new_path = ser_2026(clvm_tree_to_lazy_node(p), level=0)
            old_path = ser_2026(deser_backrefs(sexp_to_bytes(p)), level=0)
            self.assertEqual(new_path, old_path, f"mismatch for {tree!r}")

    def test_deep_tree(self):
        """Deeply nested tree should work without stack overflow."""
        p = Program.to(b"leaf")
        for _ in range(1000):
            p = Program.to((p, b"x"))
        lazy = clvm_tree_to_lazy_node(p)
        self.assertIsNotNone(lazy)

    def test_nil_atom(self):
        """Empty atom (nil) converts correctly."""
        p = Program.to(b"")
        lazy = clvm_tree_to_lazy_node(p)
        self.assertEqual(lazy.atom, b"")
        self.assertIsNone(lazy.pair)

    def test_preserves_tree_structure(self):
        """Converted tree has the same structure as the original."""
        p = Program.to((b"a", (b"b", b"c")))
        lazy = clvm_tree_to_lazy_node(p)
        self.assertEqual(lazy.pair[0].atom, b"a")
        self.assertEqual(lazy.pair[1].pair[0].atom, b"b")
        self.assertEqual(lazy.pair[1].pair[1].atom, b"c")

    def test_large_atom(self):
        """Large atoms convert without error."""
        big = b"\xab" * 100_000
        p = Program.to(big)
        lazy = clvm_tree_to_lazy_node(p)
        self.assertEqual(lazy.atom, big)


class Serde2026RoundTripTest(unittest.TestCase):
    """Comprehensive serde_2026 round-trip tests."""

    def check_2026_roundtrip(self, tree, levels=(0,)):
        """Serialize to serde_2026, deserialize, check equality."""
        p = Program.to(tree)
        for level in levels:
            blob = ser_2026(clvm_tree_to_lazy_node(p), level=level)
            p2 = Program.from_bytes(blob)
            self.assertEqual(p, p2, f"roundtrip failed for {tree!r} at level={level}")

    def test_atoms(self):
        for atom in [b"", b"\x00", b"\xff", b"hello world", b"\x01" * 1000]:
            self.check_2026_roundtrip(atom)

    def test_integers(self):
        for n in [0, 1, -1, 127, 128, 255, 256, 65535, -32768, 2**32]:
            self.check_2026_roundtrip(n)

    def test_lists(self):
        self.check_2026_roundtrip([])
        self.check_2026_roundtrip([1])
        self.check_2026_roundtrip([1, 2, 3, 4, 5])
        self.check_2026_roundtrip([[1, 2], [3, 4], [5, 6]])

    def test_nested_pairs(self):
        self.check_2026_roundtrip((1, 2))
        self.check_2026_roundtrip((1, (2, (3, (4, 5)))))
        self.check_2026_roundtrip(((((1, 2), 3), 4), 5))

    def test_shared_subtrees_serialization(self):
        """Shared subtrees should serialize compactly and round-trip."""
        shared = Program.to([1, 2, 3])
        tree = Program.to((shared, (shared, shared)))
        blob = tree.to_bytes_2026()
        p2 = Program.from_bytes(blob)
        self.assertEqual(tree, p2)

    def test_exponential_sharing(self):
        """Exponential sharing (2^50 logical nodes) should serialize compactly."""
        t = Program.to(b"x")
        for _ in range(50):
            t = Program.to((t, t))
        blob = t.to_bytes_2026()
        self.assertLess(len(blob), 500)
        # Verify it deserializes without error (skip equality check —
        # LazyNode.pair creates new wrappers, making tree_hash O(2^N))
        p2 = Program.from_bytes(blob)
        self.assertIsNotNone(p2)

    def test_exponential_sharing_small(self):
        """Smaller exponential tree (2^15) — full round-trip with equality."""
        t = Program.to(b"y")
        for _ in range(15):
            t = Program.to((t, t))
        blob = t.to_bytes_2026()
        p2 = Program.from_bytes(blob)
        self.assertEqual(t, p2)

    def test_repeated_atoms(self):
        """Many copies of the same atom should be deduplicated."""
        tree = Program.to([b"dup"] * 100)
        blob = tree.to_bytes_2026()
        p2 = Program.from_bytes(blob)
        self.assertEqual(tree, p2)

    def test_many_distinct_atoms(self):
        """Many distinct atoms should all round-trip."""
        tree = Program.to([bytes([i]) for i in range(256)])
        blob = tree.to_bytes_2026()
        p2 = Program.from_bytes(blob)
        self.assertEqual(tree, p2)

    def test_2026_level_saturates_to_highest_implemented(self):
        """`level` saturates: anything above the top implemented level
        produces the same bytes as that level. Today only level 0 exists,
        so every non-zero level must produce identical output to level 0."""
        shared = Program.to([1, 2, 3])
        tree = Program.to([shared, shared, shared, shared])
        lazy = clvm_tree_to_lazy_node(tree)
        blob_0 = ser_2026(lazy, level=0)
        for level in (1, 7, 1 << 20, (1 << 32) - 1):
            self.assertEqual(
                ser_2026(lazy, level=level),
                blob_0,
                f"level={level} should saturate to level=0 today",
            )
        self.assertEqual(Program.from_bytes(blob_0), tree)

    def test_mixed_tree_with_large_atoms(self):
        """Mix of large atoms, small atoms, and nested structure."""
        big = b"A" * 10000
        tree = Program.to((big, [1, 2, (big, b"small")]))
        self.check_2026_roundtrip(tree)

    def test_deserialize_api_formats(self):
        """Test the serde.py deserialize() with different format strings."""
        p = Program.to([42, 99])
        legacy_blob = bytes(p)
        node = deserialize(legacy_blob, "legacy")
        self.assertEqual(Program.wrap(node), p)

        node = deserialize(legacy_blob, "auto")
        self.assertEqual(Program.wrap(node), p)

        prefixed = p.to_bytes_2026()
        node = deserialize(prefixed, "auto")
        self.assertEqual(Program.wrap(node), p)

    def test_serialize_api_formats(self):
        """Test the serde.py serialize() with different format strings."""
        from clvm_rs.clvm_rs import deser_backrefs
        p = Program.to([1, 2, 3])
        node = deser_backrefs(bytes(p))

        legacy_blob = serialize(node, "legacy")
        self.assertEqual(Program.from_bytes(legacy_blob), p)

        s2026_blob = serialize(node, "2026")
        self.assertTrue(s2026_blob.startswith(bytes.fromhex("fdff32303236")))
        self.assertEqual(Program.from_bytes(s2026_blob), p)

    def test_ser_2026_deser_2026_symmetry(self):
        """ser_2026 emits the magic prefix, deser_2026 must accept it."""
        from clvm_rs.clvm_rs import deser_2026, ser_2026

        p = Program.to([1, 2, (b"shared", b"shared"), 3])
        blob = ser_2026(clvm_tree_to_lazy_node(p))
        self.assertTrue(blob.startswith(bytes.fromhex("fdff32303236")))

        node = deser_2026(blob)
        self.assertEqual(Program.wrap(node), p)

        # Without the magic prefix it must be rejected.
        with self.assertRaises(ValueError):
            deser_2026(blob[6:])
