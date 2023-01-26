from typing import Any, List, Optional, Tuple

from .at import at
from .bytes32 import bytes32
from .chia_dialect import Dialect, CHIA_DIALECT
from .clvm_storage import CLVMStorage
from .tree_hash import shatree_pair, shatree_atom


ONE = bytes.fromhex("01")


class CurryTreehasher:
    q_hw_treehash: bytes
    a_hw_treehash: bytes
    c_hw_treehash: bytes
    atom_prefix_treehash: bytes
    pair_prefix_treehash: bytes

    def __init__(self, dialect: Dialect):
        self.dialect = dialect

        self.q_kw_treehash = shatree_atom(dialect.Q_KW)
        self.c_kw_treehash = shatree_atom(dialect.C_KW)
        self.a_kw_treehash = shatree_atom(dialect.A_KW)
        self.null_treehash = shatree_atom(dialect.NULL)
        self.one_treehash = shatree_atom(ONE)

    # The environment `E = (F . R)` recursively expands out to
    # `(c . ((q . F) . EXPANSION(R)))` if R is not 0
    # `1` if R is 0

    def curried_values_tree_hash(self, arguments: List[bytes32]) -> bytes32:
        if len(arguments) == 0:
            return self.one_treehash

        inner_curried_values = self.curried_values_tree_hash(arguments[1:])

        return shatree_pair(
            self.c_kw_treehash,
            shatree_pair(
                shatree_pair(self.q_kw_treehash, arguments[0]),
                shatree_pair(inner_curried_values, self.null_treehash),
            ),
        )

    # The curry pattern is `(a . ((q . F)  . (E . 0)))` == `(a (q . F) E)
    # where `F` is the `mod` and `E` is the curried environment

    def curry_and_treehash(
        self, hash_of_quoted_mod_hash: bytes32, *hashed_arguments: bytes32
    ) -> bytes32:
        """
        `hash_of_quoted_mod_hash` : tree hash of `(q . MOD)` where `MOD`
             is template to be curried
        `arguments` : tree hashes of arguments to be curried
        """

        curried_values = self.curried_values_tree_hash(list(hashed_arguments))
        return shatree_pair(
            self.a_kw_treehash,
            shatree_pair(
                hash_of_quoted_mod_hash,
                shatree_pair(curried_values, self.null_treehash),
            ),
        )

    def calculate_hash_of_quoted_mod_hash(self, mod_hash: bytes32) -> bytes32:
        return shatree_pair(self.q_kw_treehash, mod_hash)

    """
    Replicates the curry function from clvm_tools, taking advantage of *args
    being a list.  We iterate through args in reverse building the code to
    create a clvm list.

    Given arguments to a function addressable by the '1' reference in clvm

    fixed_args = 1

    Each arg is prepended as fixed_args = (c (q . arg) fixed_args)

    The resulting argument list is interpreted with apply (2)

    (2 (1 . self) rest)

    Resulting in a function which places its own arguments after those
    curried in in the form of a proper list.
    """

    def curry(self, mod, *args) -> Any:
        fixed_args: Any = 1
        for arg in reversed(args):
            fixed_args = [self.dialect.C_KW, (self.dialect.Q_KW, arg), fixed_args]
        return [self.dialect.A_KW, (self.dialect.Q_KW, mod), fixed_args]

    """
    uncurry the given program

    returns `mod, [arg1, arg2, ...]`

    if the program is not a valid curry, return `sexp, NULL`

    This distinguishes it from the case of a valid curry of 0 arguments
    (which is rather pointless but possible), which returns `sexp, []`
    """

    def uncurry(
        self, sexp: CLVMStorage
    ) -> Tuple[CLVMStorage, Optional[List[CLVMStorage]]]:
        dialect = self.dialect
        if (
            at(sexp, "f") != dialect.A_KW
            or at(sexp, "rff") != dialect.Q_KW
            or at(sexp, "rrr") != dialect.NULL
        ):
            return sexp, None
        # since "rff" is not None, neither is "rfr"
        uncurried_function = at(sexp, "rfr")
        assert uncurried_function is not None
        core_items = []

        # since "rrr" is not None, neither is rrf
        core = at(sexp, "rrf")
        while core != dialect.ONE:
            assert core is not None
            if (
                at(core, "f") != dialect.C_KW
                or at(core, "rff") != dialect.Q_KW
                or at(core, "rrr") != dialect.NULL
            ):
                return sexp, None
            # since "rff" is not None, neither is "rfr"
            new_item = at(core, "rfr")
            assert new_item is not None
            core_items.append(new_item)
            # since "rrr" is not None, neither is rrf
            core = at(core, "rrf")
        return uncurried_function, core_items


CHIA_CURRY_TREEHASHER = CurryTreehasher(CHIA_DIALECT)
