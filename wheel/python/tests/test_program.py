from unittest import TestCase

from clvm_rs.clvm_storage import CLVMStorage
from clvm_rs.program import Program
from clvm_rs.eval_error import EvalError

from clvm_rs.keywords import A_KW, C_KW, Q_KW


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
        self.assertEqual(p.as_pair(), None)
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
    assert plus.uncurry() == (plus, Program.to(0))


def test_uncurry_args_garbage():
    # there's garbage at the end of the args list
    # `(a (q . 1) (c (q . 1) (q . 1) (q . 4919)))`
    plus = Program.fromhex("ff02ffff0101ffff04ffff0101ffff0101ffff018213378080")
    assert plus.uncurry() == (plus, Program.to(0))
