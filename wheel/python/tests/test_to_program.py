import unittest

from typing import Optional, Tuple, Any
from clvm_rs.clvm_storage import CLVMStorage
from clvm_rs.program import Program


def convert_atom_to_bytes(castable: Any) -> Optional[bytes]:
    return Program.to(castable).atom


def validate_program(program):
    validate_stack = [program]
    while validate_stack:
        v = validate_stack.pop()
        assert isinstance(v, Program)
        if v.pair:
            assert isinstance(v.pair, tuple)
            v1, v2 = v.pair
            assert isinstance(v1, CLVMStorage)
            assert isinstance(v2, CLVMStorage)
            s1, s2 = v.as_pair()
            validate_stack.append(s1)
            validate_stack.append(s2)
        else:
            assert isinstance(v.atom, bytes)


def print_leaves(tree: Program) -> str:
    a = tree.as_atom()
    if a is not None:
        if len(a) == 0:
            return "() "
        return "%d " % a[0]

    ret = ""
    for i in tree.as_pair():
        ret += print_leaves(i)

    return ret


def print_tree(tree: Program) -> str:
    a = tree.as_atom()
    if a is not None:
        if len(a) == 0:
            return "() "
        return "%d " % a[0]

    ret = "("
    for i in tree.as_pair():
        ret += print_tree(i)
    ret += ")"
    return ret


class ToProgramTest(unittest.TestCase):
    def test_cast_1(self):
        # this was a problem in `clvm_tools` and is included
        # to prevent regressions
        program = Program.to(b"foo")
        t1 = program.to([1, program])
        validate_program(t1)

    def test_wrap_program(self):
        # it's a bit of a layer violation that CLVMStorage unwraps Program, but we
        # rely on that in a fair number of places for now. We should probably
        # work towards phasing that out
        o = Program.to(Program.to(1))
        assert o.atom == bytes([1])

    def test_arbitrary_underlying_tree(self):

        # Program provides a view on top of a tree of arbitrary types, as long as
        # those types implement the CLVMStorage protocol. This is an example of
        # a tree that's generated
        class GeneratedTree:

            depth: int = 4
            val: int = 0

            def __init__(self, depth, val):
                assert depth >= 0
                self.depth = depth
                self.val = val
                self._cached_sha256_treehash = None

            @property
            def atom(self) -> Optional[bytes]:
                if self.depth > 0:
                    return None
                return bytes([self.val])

            @property
            def pair(self) -> Optional[Tuple[Any, Any]]:
                if self.depth == 0:
                    return None
                new_depth: int = self.depth - 1
                return (
                    GeneratedTree(new_depth, self.val),
                    GeneratedTree(new_depth, self.val + 2**new_depth),
                )

        tree = Program.to(GeneratedTree(5, 0))
        assert (
            print_leaves(tree)
            == "0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 "
            + "16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 "
        )

        tree = Program.to(GeneratedTree(3, 0))
        assert print_leaves(tree) == "0 1 2 3 4 5 6 7 "

        tree = Program.to(GeneratedTree(3, 10))
        assert print_leaves(tree) == "10 11 12 13 14 15 16 17 "

        # this is just for `coverage`
        assert print_leaves(Program.to(0)) == "() "

    def test_looks_like_clvm_object(self):

        # this function can't look at the values, that would cause a cascade of
        # eager evaluation/conversion
        class dummy:
            pass

        obj = dummy()
        obj.atom = None
        obj.pair = None
        obj._cached_sha256_treehash = None
        print(dir(obj))
        assert isinstance(obj, CLVMStorage)

        obj = dummy()
        obj.pair = None
        obj._cached_sha256_treehash = None
        assert not isinstance(obj, CLVMStorage)

        obj = dummy()
        obj.atom = None
        obj._cached_sha256_treehash = None
        assert not isinstance(obj, CLVMStorage)

        obj = dummy()
        obj.atom = None
        obj.pair = None
        assert not isinstance(obj, CLVMStorage)

    def test_list_conversions(self):
        a = Program.to([1, 2, 3])
        assert print_tree(a) == "(1 (2 (3 () )))"

    def test_string_conversions(self):
        a = Program.to("foobar")
        assert a.as_atom() == "foobar".encode()

    def test_int_conversions(self):
        a = Program.to(1337)
        assert a.as_atom() == bytes([0x5, 0x39])

    def test_none_conversions(self):
        a = Program.to(None)
        assert a.as_atom() == b""

    def test_empty_list_conversions(self):
        a = Program.to([])
        assert a.as_atom() == b""

    def test_eager_conversion(self):
        with self.assertRaises(ValueError):
            Program.to(("foobar", (1, {})))

    def test_convert_atom(self):
        assert convert_atom_to_bytes(0x133742) == bytes([0x13, 0x37, 0x42])
        assert convert_atom_to_bytes(0x833742) == bytes([0x00, 0x83, 0x37, 0x42])
        assert convert_atom_to_bytes(0) == b""

        assert convert_atom_to_bytes("foobar") == "foobar".encode()
        assert convert_atom_to_bytes("") == b""

        assert convert_atom_to_bytes(b"foobar") == b"foobar"
        assert convert_atom_to_bytes(None) == b""
        assert convert_atom_to_bytes([]) == b""

        assert convert_atom_to_bytes([1, 2, 3]) is None

        assert convert_atom_to_bytes((1, 2)) is None

        with self.assertRaises(ValueError):
            assert convert_atom_to_bytes({})

    def test_to_nil(self):
        self.assertEqual(Program.to([]), 0)
        self.assertEqual(Program.to(0), 0)
        self.assertEqual(Program.to(b""), 0)
