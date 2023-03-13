from typing import Optional, Tuple, Any, Union

from unittest import TestCase

from clvm_rs.clvm_storage import CLVMStorage, is_clvm_storage
from clvm_rs.chia_dialect import CHIA_DIALECT
from clvm_rs.eval_error import EvalError
from clvm_rs.program import Program

A_KW, C_KW, Q_KW = [getattr(CHIA_DIALECT, _) for _ in "A_KW C_KW Q_KW".split()]

Program.set_run_unsafe_max_cost(0x7FFFFFFFFFFFFFFF)


class TestProgram(TestCase):
    def test_at(self):
        p = Program.to([10, 20, 30, [15, 17], 40, 50])

        self.assertEqual(p.first(), p.at("f"))
        self.assertEqual(Program.to(10), p.at("f"))

        self.assertEqual(p.rest(), p.at("r"))
        self.assertEqual(Program.to([20, 30, [15, 17], 40, 50]), p.at("r"))

        self.assertEqual(p.rest().rest().rest().first().rest().first(), p.at("rrrfrf"))
        self.assertEqual(Program.to(17), p.at("rrrfrf"))

        self.assertRaises(ValueError, lambda: p.at("q"))
        self.assertEqual(None, p.at("ff"))
        self.assertEqual(None, p.at("ffr"))

    def test_at_many(self):
        p = Program.to([10, 20, 30, [15, 17], 40, 50])
        self.assertEqual(p.at_many("f", "rrrfrf"), [10, 17])
        self.assertEqual(p.at_many("fff", "rrrfff"), [None, None])

    def test_replace(self):
        p1 = Program.to([100, 200, 300])
        self.assertEqual(p1.replace(f=105), Program.to([105, 200, 300]))
        self.assertEqual(p1.replace(rrf=[301, 302]), Program.to([100, 200, [301, 302]]))
        self.assertEqual(
            p1.replace(f=105, rrf=[301, 302]), Program.to([105, 200, [301, 302]])
        )
        self.assertEqual(p1.replace(f=100, r=200), Program.to((100, 200)))

    def test_replace_conflicts(self):
        p1 = Program.to([100, 200, 300])
        self.assertRaises(ValueError, lambda: p1.replace(rr=105, rrf=200))

    def test_replace_conflicting_paths(self):
        p1 = Program.to([100, 200, 300])
        self.assertRaises(ValueError, lambda: p1.replace(ff=105))

    def test_replace_bad_path(self):
        p1 = Program.to([100, 200, 300])
        self.assertRaises(ValueError, lambda: p1.replace(q=105))
        self.assertRaises(ValueError, lambda: p1.replace(rq=105))

    def test_first_rest(self):
        p = Program.to([4, 5])
        self.assertEqual(p.first(), 4)
        self.assertEqual(p.rest(), [5])
        p = Program.to(4)
        self.assertEqual(p.pair, None)
        self.assertEqual(p.first(), None)
        self.assertEqual(p.rest(), None)

    def test_simple_run(self):
        p = Program.fromhex("ff10ff02ff0580")  # `(+ 2 5)`
        args = Program.fromhex("ff32ff3c80")  # `(50 60)`
        r = p.run(args)
        self.assertEqual(r, 110)

    def test_run_exception(self):
        p = Program.fromhex(
            "ff08ffff0183666f6fffff018362617280"
        )  # `(x (q . foo) (q . bar))`
        err = None
        try:
            p.run(p)
        except EvalError as ee:
            err = ee
        self.assertEqual(err.args, ("clvm raise",))
        self.assertEqual(err._sexp, ["foo", "bar"])

    def test_hash(self):
        p1 = Program.fromhex("80")
        assert hash(p1) == id(p1)

    def test_long_repr(self):
        p1 = Program.fromhex(f"c062{'61'*98}")
        assert repr(p1) == f"Program(c062{'61'*33}...616161)"


def check_idempotency(p, *args):
    curried = p.curry(*args)

    f_0, args_0 = curried.uncurry()

    assert f_0 == p
    assert len(args_0) == len(args)
    for a, a0 in zip(args, args_0):
        assert a == a0

    return curried


def test_curry_uncurry():
    PLUS = Program.fromhex("10")  # `+`

    p = Program.fromhex("ff10ff02ff0580")  # `(+ 2 5)`

    curried_p = check_idempotency(p)
    assert curried_p == [A_KW, [Q_KW, PLUS, 2, 5], 1]

    curried_p = check_idempotency(p, b"dogs")
    assert curried_p == [A_KW, [Q_KW, PLUS, 2, 5], [C_KW, (Q_KW, "dogs"), 1]]

    curried_p = check_idempotency(p, 200, 30)
    assert curried_p == [
        A_KW,
        [Q_KW, PLUS, 2, 5],
        [C_KW, (Q_KW, 200), [C_KW, (Q_KW, 30), 1]],
    ]

    # passing "args" here wraps the arguments in a list
    curried_p = check_idempotency(p, 50, 60, 70, 80)
    assert curried_p == [
        A_KW,
        [Q_KW, PLUS, 2, 5],
        [
            C_KW,
            (Q_KW, 50),
            [C_KW, (Q_KW, 60), [C_KW, (Q_KW, 70), [C_KW, (Q_KW, 80), 1]]],
        ],
    ]


def test_uncurry_not_curried():
    # this function has not been curried
    plus = Program.fromhex("ff10ff02ff0580")  # `(+ 2 5)`
    assert plus.uncurry() == (plus, None)


def test_uncurry():
    # this is a positive test
    # `(a (q . (+ 2 5)) (c (q . 1) 1))`
    plus = Program.fromhex("ff02ffff01ff10ff02ff0580ffff04ffff0101ff018080")
    prog = Program.fromhex("ff10ff02ff0580")  # `(+ 2 5)`
    args = Program.fromhex("01")  # `1`
    assert plus.uncurry() == (prog, [args])


def test_uncurry_top_level_garbage():
    # there's garbage at the end of the top-level list
    # `(a (q . 1) (c (q . 1) (q . 1)) (q . 0x1337))`
    plus = Program.fromhex("ff02ffff0101ffff04ffff0101ffff010180ffff0182133780")
    assert plus.uncurry() == (plus, None)


def test_uncurry_not_pair():
    # the second item in the list is expected to be a pair, with a qoute
    # `(a 1 (c (q . 1) (q . 1)))`
    plus = Program.fromhex("ff02ff01ffff04ffff0101ffff01018080")
    assert plus.uncurry() == (plus, None)


def test_uncurry_args_garbage():
    # there's garbage at the end of the args list
    # `(a (q . 1) (c (q . 1) (q . 1) (q . 4919)))`
    plus = Program.fromhex("ff02ffff0101ffff04ffff0101ffff0101ffff018213378080")
    assert plus.uncurry() == (plus, None)


class SimpleStorage(CLVMStorage):
    """
    A simple implementation of `CLVMStorage`.
    """

    atom: Optional[bytes]

    def __init__(self, atom, pair):
        self.atom = atom
        self._pair = pair

    @property
    def pair(self) -> Optional[Tuple["CLVMStorage", "CLVMStorage"]]:
        return self._pair


class Uncachable(SimpleStorage):
    """
    This object does not allow `_cached_sha256_treehash` or `_cached_serialization`
    to be stored.
    """

    def get_th(self):
        return None

    def set_th(self, v):
        raise AttributeError("can't set property")

    _cached_sha256_treehash = property(get_th, set_th)
    _cached_serialization = property(get_th, set_th)


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
            assert is_clvm_storage(v1)
            assert is_clvm_storage(v2)
            s1, s2 = v.pair
            validate_stack.append(s1)
            validate_stack.append(s2)
        else:
            assert isinstance(v.atom, bytes)


def print_leaves(tree: Program) -> str:
    a = tree.atom
    if a is not None:
        if len(a) == 0:
            return "() "
        return "%d " % a[0]

    ret = ""
    pair = tree.pair
    assert pair is not None
    for i in pair:
        ret += print_leaves(i)

    return ret


def print_tree(tree: Program) -> str:
    a = tree.atom
    if a is not None:
        if len(a) == 0:
            return "() "
        return "%d " % a[0]

    pair = tree.pair
    assert pair is not None
    ret = "("
    for i in pair:
        ret += print_tree(i)
    ret += ")"
    return ret


class ProgramTest(TestCase):
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

    def test_arbitrary_underlying_tree(self) -> None:
        # Program provides a view on top of a tree of arbitrary types, as long as
        # those types implement the CLVMStorage protocol. This is an example of
        # a tree that's generated
        class GeneratedTree(CLVMStorage):
            depth: int = 4
            val: int = 0

            def __init__(self, depth, val):
                assert depth >= 0
                self.depth = depth
                self.val = val
                self._cached_sha256_treehash = None
                self.atom = None if self.depth > 0 else bytes([self.val])

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
        print(dir(obj))
        assert is_clvm_storage(obj)

        obj = dummy()
        obj.pair = None
        assert not is_clvm_storage(obj)

        obj = dummy()
        obj.atom = None
        assert not is_clvm_storage(obj)

    def test_list_conversions(self):
        a = Program.to([1, 2, 3])
        assert print_tree(a) == "(1 (2 (3 () )))"

    def test_string_conversions(self):
        a = Program.to("foobar")
        assert a.atom == "foobar".encode()

    def test_int_conversions(self):
        def check(v: int, h: Union[str, list]):
            a = Program.to(v)
            b = bytes.fromhex(h) if isinstance(h, str) else bytes(h)
            assert a.atom == b
            # note that this compares to the atom, not the serialization of that atom
            # so 16384 codes as 0x4000, not 0x824000

        check(1337, "0539")
        check(-128, "80")
        check(0, "")
        check(1, "01")
        check(-1, "ff")

        for v in range(1, 0x80):
            check(v, [v])

        for v in range(0x80, 0xFF):
            check(v, [0, v])

        for v in range(128):
            check(-v - 1, [255 - v])

        check(127, "7f")
        check(128, "0080")
        check(256, "0100")
        check(-256, "ff00")
        check(16384, "4000")
        check(32767, "7fff")
        check(32768, "008000")
        check(-32768, "8000")
        check(-32769, "ff7fff")

    def test_int_round_trip(self):
        def check(n):
            p = Program.to(n)
            assert int(p) == n
            assert p.int_from_bytes(p.atom) == n
            assert Program.int_to_bytes(n) == p.atom

        for n in range(0, 256):
            check(n)
            check(-n)

        for n in range(0, 65536, 97):
            check(n)
            check(-n)

    def test_empty_list_conversions(self):
        a = Program.to([])
        assert a.atom == b""

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
        assert convert_atom_to_bytes([]) == b""

        assert convert_atom_to_bytes([1, 2, 3]) is None

        assert convert_atom_to_bytes((1, 2)) is None

        with self.assertRaises(ValueError):
            assert convert_atom_to_bytes({})

    def test_to_nil(self):
        self.assertEqual(Program.to([]), 0)
        self.assertEqual(Program.to(0), 0)
        self.assertEqual(Program.to(b""), 0)

    def test_tree_hash_caching(self):
        o = SimpleStorage(b"foo", None)
        eh = "0080b50a51ecd0ccfaaa4d49dba866fe58724f18445d30202bafb03e21eef6cb"
        p = Program.to(o)
        self.assertEqual(p.tree_hash().hex(), eh)
        self.assertEqual(p._cached_sha256_treehash.hex(), eh)
        self.assertEqual(o._cached_sha256_treehash.hex(), eh)

        o2 = SimpleStorage(None, (o, o))
        eh2 = "4a40c538671ef10c8d956e5dd3625e167c8adfb666c943f67f91ea58fd7a302c"
        p2 = Program.to(o2)
        self.assertEqual(p2.tree_hash().hex(), eh2)
        self.assertEqual(p2._cached_sha256_treehash.hex(), eh2)
        self.assertEqual(o._cached_sha256_treehash.hex(), eh)
        self.assertEqual(o2._cached_sha256_treehash.hex(), eh2)

        p2p = Program.to((p, p))
        self.assertEqual(p2p.tree_hash().hex(), eh2)
        self.assertEqual(p2p._cached_sha256_treehash.hex(), eh2)
        self.assertEqual(p._cached_sha256_treehash.hex(), eh)
        self.assertEqual(p2._cached_sha256_treehash.hex(), eh2)

        o3 = SimpleStorage(None, (o2, o2))
        eh3 = "280df61ed70cac1ec3cf9811c15f75e6698516b0354252960a62fa31240e4970"
        p3 = Program.to(o3)
        self.assertEqual(p3.tree_hash().hex(), eh3)
        self.assertEqual(p3._cached_sha256_treehash.hex(), eh3)
        self.assertEqual(o._cached_sha256_treehash.hex(), eh)
        self.assertEqual(o2._cached_sha256_treehash.hex(), eh2)
        self.assertEqual(o3._cached_sha256_treehash.hex(), eh3)

        p3p = Program.to((p2, p2))
        self.assertEqual(p3p.tree_hash().hex(), eh3)
        self.assertEqual(p3p._cached_sha256_treehash.hex(), eh3)
        self.assertEqual(p._cached_sha256_treehash.hex(), eh)
        self.assertEqual(p2._cached_sha256_treehash.hex(), eh2)

    def test_tree_hash_no_caching(self):
        o = Uncachable(b"foo", None)
        eh = "0080b50a51ecd0ccfaaa4d49dba866fe58724f18445d30202bafb03e21eef6cb"
        p = Program.to(o)
        self.assertEqual(p.tree_hash().hex(), eh)
        self.assertEqual(p._cached_sha256_treehash.hex(), eh)
        self.assertEqual(o._cached_sha256_treehash, None)

        o2 = Uncachable(None, (o, o))
        eh2 = "4a40c538671ef10c8d956e5dd3625e167c8adfb666c943f67f91ea58fd7a302c"
        p2 = Program.to(o2)
        self.assertEqual(p2.tree_hash().hex(), eh2)
        self.assertEqual(p2._cached_sha256_treehash.hex(), eh2)
        self.assertEqual(o._cached_sha256_treehash, None)
        self.assertEqual(o2._cached_sha256_treehash, None)

        p2p = Program.to((p, p))
        self.assertEqual(p2p.tree_hash().hex(), eh2)
        self.assertEqual(p2p._cached_sha256_treehash.hex(), eh2)
        self.assertEqual(p._cached_sha256_treehash.hex(), eh)
        self.assertEqual(p2._cached_sha256_treehash.hex(), eh2)

        o3 = Uncachable(None, (o2, o2))
        eh3 = "280df61ed70cac1ec3cf9811c15f75e6698516b0354252960a62fa31240e4970"
        p3 = Program.to(o3)
        self.assertEqual(p3.tree_hash().hex(), eh3)
        self.assertEqual(p3._cached_sha256_treehash.hex(), eh3)
        self.assertEqual(o._cached_sha256_treehash, None)
        self.assertEqual(o2._cached_sha256_treehash, None)
        self.assertEqual(o3._cached_sha256_treehash, None)

        p3p = Program.to((p2, p2))
        self.assertEqual(p3p.tree_hash().hex(), eh3)
        self.assertEqual(p3p._cached_sha256_treehash.hex(), eh3)
        self.assertEqual(p._cached_sha256_treehash.hex(), eh)
        self.assertEqual(p2._cached_sha256_treehash.hex(), eh2)
