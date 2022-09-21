import unittest

from clvm_rs.program import Program

from blspy import G1Element


class dummy_class:
    def __init__(self):
        self.i = 0


def gen_tree(depth: int) -> Program:
    if depth == 0:
        return Program.to(1337)
    subtree = gen_tree(depth - 1)
    return Program.to((subtree, subtree))


fh = bytes.fromhex
H01 = fh("01")
H02 = fh("02")


class AsPythonTest(unittest.TestCase):
    def check_as_atom_list(self, p):
        v = Program.to(p)
        p1 = v.as_atom_list()
        self.assertEqual(p, p1)

    def test_null(self):
        self.check_as_atom_list([])

    def test_single_bytes(self):
        for _ in range(256):
            self.check_as_atom_list([bytes([_])])

    def test_short_lists(self):
        self.check_as_atom_list([])
        for _ in range(256):
            for size in range(1, 5):
                self.check_as_atom_list([bytes([_])] * size)

    def test_int(self):
        v = Program.to(42)
        self.assertEqual(v.atom, bytes([42]))

    def test_none(self):
        v = Program.to(None)
        self.assertEqual(v.atom, b"")

    def test_empty_list(self):
        v = Program.to([])
        self.assertEqual(v.atom, b"")

    def test_list_of_one(self):
        v = Program.to([1])
        self.assertEqual(type(v.pair[0]), Program)
        self.assertEqual(type(v.pair[1]), Program)
        self.assertEqual(type(v.as_pair()[0]), Program)
        self.assertEqual(type(v.as_pair()[1]), Program)
        self.assertEqual(v.pair[0].atom, b"\x01")
        self.assertEqual(v.pair[1].atom, b"")

    def test_g1element(self):
        b = fh(
            "b3b8ac537f4fd6bde9b26221d49b54b17a506be147347dae5"
            "d081c0a6572b611d8484e338f3432971a9823976c6a232b"
        )
        v = Program.to(G1Element(b))
        self.assertEqual(v.atom, b)

    def test_complex(self):
        self.check_as_atom_list([b"foo"])
        self.check_as_atom_list([b"2", b"1"])
        self.check_as_atom_list([b"", b"2", b"1"])
        self.check_as_atom_list([b"", b"1", b"2", b"30", b"40", b"90", b"600"])

    def test_listp(self):
        self.assertEqual(Program.to(42).listp(), False)
        self.assertEqual(Program.to(b"").listp(), False)
        self.assertEqual(Program.to(b"1337").listp(), False)

        self.assertEqual(Program.to((1337, 42)).listp(), True)
        self.assertEqual(Program.to([1337, 42]).listp(), True)

    def test_nullp(self):
        self.assertEqual(Program.to(b"").nullp(), True)
        self.assertEqual(Program.to(b"1337").nullp(), False)
        self.assertEqual(Program.to((b"", b"")).nullp(), False)

    def test_constants(self):
        self.assertEqual(Program.null().nullp(), True)

    def test_list_len(self):
        v = Program.to(42)
        for i in range(100):
            self.assertEqual(v.list_len(), i)
            v = Program.to((42, v))
        self.assertEqual(v.list_len(), 100)

    def test_list_len_atom(self):
        v = Program.to(42)
        self.assertEqual(v.list_len(), 0)

    def test_as_int(self):
        self.assertEqual(Program.to(fh("80")).as_int(), -128)
        self.assertEqual(Program.to(fh("ff")).as_int(), -1)
        self.assertEqual(Program.to(fh("0080")).as_int(), 128)
        self.assertEqual(Program.to(fh("00ff")).as_int(), 255)

    def test_string(self):
        self.assertEqual(Program.to("foobar").as_atom(), b"foobar")

    def test_deep_recursion(self):
        d = b"2"
        for i in range(1000):
            d = [d]
        v = Program.to(d)
        for i in range(1000):
            self.assertEqual(v.as_pair()[1].as_atom(), Program.null())
            v = v.as_pair()[0]
            d = d[0]

        self.assertEqual(v.as_atom(), b"2")
        self.assertEqual(d, b"2")

    def test_long_linked_list(self):
        d = b""
        for i in range(1000):
            d = (b"2", d)
        v = Program.to(d)
        for i in range(1000):
            self.assertEqual(v.as_pair()[0].as_atom(), d[0])
            v = v.as_pair()[1]
            d = d[1]

        self.assertEqual(v.as_atom(), b"")
        self.assertEqual(d, b"")

    def test_long_list(self):
        d = [1337] * 1000
        v = Program.to(d)
        for i in range(1000):
            self.assertEqual(v.as_pair()[0].as_int(), d[i])
            v = v.as_pair()[1]

        self.assertEqual(v.as_atom(), b"")

    def test_invalid_tuple(self):
        with self.assertRaises(ValueError):
            s = Program.to((dummy_class, dummy_class))

        with self.assertRaises(ValueError):
            s = Program.to((dummy_class, dummy_class, dummy_class))

    def test_clvm_object_tuple(self):
        o1 = Program.to(b"foo")
        o2 = Program.to(b"bar")
        self.assertEqual(Program.to((o1, o2)), (o1, o2))

    def test_first(self):
        val = Program.to(1)
        self.assertEqual(val.first(), None)
        val = Program.to((42, val))
        self.assertEqual(val.first(), Program.to(42))

    def test_rest(self):
        val = Program.to(1)
        self.assertEqual(val.first(), None)
        val = Program.to((42, val))
        self.assertEqual(val.rest(), Program.to(1))

    def test_as_iter(self):
        val = list(Program.to((1, (2, (3, (4, b""))))).as_iter())
        self.assertEqual(val, [1, 2, 3, 4])

        val = list(Program.to(b"").as_iter())
        self.assertEqual(val, [])

        val = list(Program.to((1, b"")).as_iter())
        self.assertEqual(val, [1])

        # these fail because the lists are not null-terminated
        self.assertEqual(list(Program.to(1).as_iter()), [])
        self.assertEqual(list(Program.to((1, (2, (3, (4, 5))))).as_iter()), [1, 2, 3, 4])

    def test_eq(self):
        val = Program.to(1)

        self.assertTrue(val == 1)
        self.assertFalse(val == 2)

        # mismatching types
        self.assertFalse(val == [1])
        self.assertFalse(val == [1, 2])
        self.assertFalse(val == (1, 2))
        self.assertRaises(ValueError, lambda: val == (dummy_class, dummy_class))

    def test_eq_tree(self):
        val1 = gen_tree(2)
        val2 = gen_tree(2)
        val3 = gen_tree(3)

        self.assertTrue(val1 == val2)
        self.assertTrue(val2 == val1)
        self.assertFalse(val1 == val3)
        self.assertFalse(val3 == val1)

    def test_str(self):
        self.assertEqual(str(Program.to(1)), "01")
        self.assertEqual(str(Program.to(1337)), "820539")
        self.assertEqual(str(Program.to(-1)), "81ff")
        self.assertEqual(str(gen_tree(1)), "ff820539820539")
        self.assertEqual(str(gen_tree(2)), "ffff820539820539ff820539820539")

    def test_repr(self):
        self.assertEqual(repr(Program.to(1)), "Program(01)")
        self.assertEqual(repr(Program.to(1337)), "Program(820539)")
        self.assertEqual(repr(Program.to(-1)), "Program(81ff)")
        self.assertEqual(repr(gen_tree(1)), "Program(ff820539820539)")
        self.assertEqual(repr(gen_tree(2)), "Program(ffff820539820539ff820539820539)")
