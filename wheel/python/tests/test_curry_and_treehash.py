from clvm_rs.program import Program
from clvm_rs.curry_and_treehash import (
    calculate_hash_of_quoted_mod_hash,
    curry_and_treehash,
)


def test_curry_and_treehash() -> None:

    arbitrary_mod = Program.fromhex("ff10ff02ff0580")  # `(+ 2 5)`
    arbitrary_mod_hash = arbitrary_mod.tree_hash()

    # we don't really care what `arbitrary_mod` is. We just need some code

    quoted_mod_hash = calculate_hash_of_quoted_mod_hash(arbitrary_mod_hash)

    for v in range(500):
        args = [v, v * v, v * v * v]
        # we don't really care about the arguments either
        puzzle = arbitrary_mod.curry(*args)
        puzzle_hash_via_curry = puzzle.tree_hash()
        hashed_args = [Program.to(_).tree_hash() for _ in args]
        puzzle_hash_via_f = curry_and_treehash(quoted_mod_hash, *hashed_args)
        assert puzzle_hash_via_curry == puzzle_hash_via_f
