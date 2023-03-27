import pytest

from clvm_rs import Program
from clvm_rs.chia_dialect import CHIA_DIALECT
from clvm_rs.curry_and_treehash import CurryTreehasher

CHIA_CURRY_TREEHASHER = CurryTreehasher(CHIA_DIALECT)
curry_and_treehash = CHIA_CURRY_TREEHASHER.curry_and_treehash
calculate_hash_of_quoted_mod_hash = (
    CHIA_CURRY_TREEHASHER.calculate_hash_of_quoted_mod_hash
)


def test_curry_and_treehash() -> None:
    arbitrary_mod = Program.fromhex("ff10ff02ff0580")  # `(+ 2 5)`
    arbitrary_mod_hash = arbitrary_mod.tree_hash()

    # we don't really care what `arbitrary_mod` is. We just need some code

    quoted_mod_hash = calculate_hash_of_quoted_mod_hash(arbitrary_mod_hash)
    exp_hash = "9f487f9078d4b215e0cbe2cbdd21215ad6ed8e894ae00d616751e0efdccb25a9"
    assert quoted_mod_hash == bytes.fromhex(exp_hash)

    for v in range(500):
        args = [v, v * v, v * v * v]
        # we don't really care about the arguments either
        puzzle = arbitrary_mod.curry(*args)
        puzzle_hash_via_curry = puzzle.tree_hash()
        hashed_args = [Program.to(_).tree_hash() for _ in args]
        puzzle_hash_via_f = curry_and_treehash(quoted_mod_hash, *hashed_args)
        assert puzzle_hash_via_curry == puzzle_hash_via_f
        puzzle_hash_via_m = arbitrary_mod.curry_hash(*hashed_args)
        assert puzzle_hash_via_curry == puzzle_hash_via_m


def test_bad_parameter() -> None:
    arbitrary_mod = Program.fromhex("ff10ff02ff0580")  # `(+ 2 5)`
    with pytest.raises(ValueError):
        arbitrary_mod.curry_hash(b"foo")
