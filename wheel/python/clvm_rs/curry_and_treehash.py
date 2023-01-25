from typing import List

from .bytes32 import bytes32
from .chia_dialect import Dialect, chia_dialect
from .tree_hash import shatree_pair, Treehash, CHIA_TREEHASHER


ONE = bytes.fromhex("01")


class CurryTreehasher:
    q_hw_treehash: bytes
    a_hw_treehash: bytes
    c_hw_treehash: bytes
    atom_prefix_treehash: bytes
    pair_prefix_treehash: bytes

    def __init__(self, dialect: Dialect, tree_hash: Treehash):
        self.dialect = dialect
        self.tree_hash = tree_hash

        self.q_kw_treehash = self.tree_hash.shatree_atom(dialect.Q_KW)
        self.c_kw_treehash = self.tree_hash.shatree_atom(dialect.C_KW)
        self.a_kw_treehash = self.tree_hash.shatree_atom(dialect.A_KW)
        self.null_treehash = self.tree_hash.shatree_atom(dialect.NULL)
        self.one_treehash = self.tree_hash.shatree_atom(ONE)

    # The environment `E = (F . R)` recursively expands out to
    # `(c . ((q . F) . EXPANSION(R)))` if R is not 0
    # `1` if R is 0

    def curried_values_tree_hash(self, arguments: List[bytes32]) -> bytes32:
        if len(arguments) == 0:
            return self.one_treehash

        inner_curried_values = self.curried_values_tree_hash(arguments[1:])

        return self.tree_hash.shatree_pair(
            self.c_kw_treehash,
            self.tree_hash.shatree_pair(
                self.tree_hash.shatree_pair(self.q_kw_treehash, arguments[0]),
                self.tree_hash.shatree_pair(inner_curried_values, self.null_treehash),
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


CHIA_CURRY_TREEHASHER = CurryTreehasher(chia_dialect, CHIA_TREEHASHER)

curry_and_treehash = CHIA_CURRY_TREEHASHER.curry_and_treehash
calculate_hash_of_quoted_mod_hash = (
    CHIA_CURRY_TREEHASHER.calculate_hash_of_quoted_mod_hash
)
