import io
import unittest

from clvm_rs.program import Program
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

        o = Program.fromhex("ff808185")
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
