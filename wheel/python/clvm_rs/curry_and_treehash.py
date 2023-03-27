from typing import List, Optional, Tuple

from .at import at
from .casts import CastableType
from .chia_dialect import Dialect
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
        # cache some frequently used values
        self.q_kw_treehash = shatree_atom(dialect.Q_KW)
        self.c_kw_treehash = shatree_atom(dialect.C_KW)
        self.a_kw_treehash = shatree_atom(dialect.A_KW)
        self.null_treehash = shatree_atom(dialect.NULL)
        self.one_treehash = shatree_atom(ONE)

    def curried_values_tree_hash(self, arguments: List[bytes]) -> bytes:
        """
        Given a list of hashes of arguments, calculate the so-called
        "curried values tree hash" needed by `curry_and_treehash` below.

        The environment `E = (F . R)` recursively expands out to
            `(c . ((q . F) . EXPANSION(R)))` if R is not 0
            `1` if R is 0
        """

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

    def curry_and_treehash(
        self, hash_of_quoted_mod_hash: bytes, *hashed_arguments: bytes
    ) -> bytes:
        """
        Return the hash of a function whose quoted mod hash is
        `hash_of_quoted_mod_hash` when curried with the arguments whose
        hashes are in `hashed_arguments`.

        `hash_of_quoted_mod_hash` : tree hash of `(q . MOD)` where `MOD`
             is template to be curried
        `arguments` : tree hashes of arguments to be curried
        """

        # The curry pattern is `(a . ((q . F)  . (E . 0)))` == `(a (q . F) E)
        # where `F` is the `mod` and `E` is the curried environment

        for arg in hashed_arguments:
            if not isinstance(arg, bytes) or len(arg) != 32:
                raise ValueError(f"arguments must be bytes of len 32: {arg.hex()}")

        curried_values = self.curried_values_tree_hash(list(hashed_arguments))
        return shatree_pair(
            self.a_kw_treehash,
            shatree_pair(
                hash_of_quoted_mod_hash,
                shatree_pair(curried_values, self.null_treehash),
            ),
        )

    def calculate_hash_of_quoted_mod_hash(self, mod_hash: bytes) -> bytes:
        """
        Calculate the hash of `(q mod_hash)`, as it's a common subexpression used
        in `curry_and_treehash` and might be worth caching.
        """
        return shatree_pair(self.q_kw_treehash, mod_hash)

    def curry(self, mod: CLVMStorage, *args: CastableType) -> CastableType:
        """
        Curry a mod template with the given args, returning a new function.

        We iterate through args in reverse building the code to create
        a clvm list.

        Given arguments to a function addressable by the '1' reference in clvm

        `fixed_args = 1`

        Each arg is prepended as fixed_args = (c (q . arg) fixed_args)

        The resulting argument list is interpreted with `a` (for "apply")

        `(a (q . self) rest)`

        Resulting in a function which places its own arguments after those
        curried in in the form of a proper list.
        """

        fixed_args: CastableType = 1
        for arg in reversed(args):
            fixed_args = [self.dialect.C_KW, (self.dialect.Q_KW, arg), fixed_args]
        return [self.dialect.A_KW, (self.dialect.Q_KW, mod), fixed_args]

    def uncurry(
        self, sexp: CLVMStorage
    ) -> Tuple[CLVMStorage, Optional[List[CLVMStorage]]]:
        """
        uncurry the given program

        returns `mod, [arg1, arg2, ...]`

        if the program is not a valid curry, return `sexp, NULL`

        This distinguishes it from the case of a valid curry of 0 arguments
        (which is rather pointless but possible), which returns `sexp, []`
        """

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
