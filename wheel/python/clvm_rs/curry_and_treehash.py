from typing import List

from .bytes32 import bytes32
from .chia_dialect import NULL, ONE, Q_KW, A_KW, C_KW
from .tree_hash import shatree_atom, shatree_pair


Q_KW_TREEHASH = shatree_atom(Q_KW)
A_KW_TREEHASH = shatree_atom(A_KW)
C_KW_TREEHASH = shatree_atom(C_KW)
ONE_TREEHASH = shatree_atom(ONE)
NULL_TREEHASH = shatree_atom(NULL)


# The environment `E = (F . R)` recursively expands out to
# `(c . ((q . F) . EXPANSION(R)))` if R is not 0
# `1` if R is 0


def curried_values_tree_hash(arguments: List[bytes32]) -> bytes32:
    if len(arguments) == 0:
        return ONE_TREEHASH

    inner_curried_values = curried_values_tree_hash(arguments[1:])

    return shatree_pair(
        C_KW_TREEHASH,
        shatree_pair(
            shatree_pair(Q_KW_TREEHASH, arguments[0]),
            shatree_pair(inner_curried_values, NULL_TREEHASH),
        ),
    )


# The curry pattern is `(a . ((q . F)  . (E . 0)))` == `(a (q . F) E)
# where `F` is the `mod` and `E` is the curried environment


def curry_and_treehash(
    hash_of_quoted_mod_hash: bytes32, *hashed_arguments: bytes32
) -> bytes32:
    """
    `hash_of_quoted_mod_hash` : tree hash of `(q . MOD)` where `MOD`
         is template to be curried
    `arguments` : tree hashes of arguments to be curried
    """

    curried_values = curried_values_tree_hash(list(hashed_arguments))
    return shatree_pair(
        A_KW_TREEHASH,
        shatree_pair(
            hash_of_quoted_mod_hash,
            shatree_pair(curried_values, NULL_TREEHASH),
        ),
    )


def calculate_hash_of_quoted_mod_hash(mod_hash: bytes32) -> bytes32:
    return shatree_pair(Q_KW_TREEHASH, mod_hash)
