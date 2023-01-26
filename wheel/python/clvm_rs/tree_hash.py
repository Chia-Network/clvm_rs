"""
This is an implementation of `sha256_treehash`, used to calculate
puzzle hashes in clvm.

This implementation goes to great pains to be non-recursive so we don't
have to worry about blowing out the python stack.
"""

from hashlib import sha256
from typing import List
from weakref import WeakKeyDictionary

from .clvm_storage import CLVMStorage

bytes32 = bytes


class Treehasher:
    atom_prefix: bytes
    pair_prefix: bytes

    def __init__(self, atom_prefix: bytes, pair_prefix: bytes):
        self.atom_prefix = atom_prefix
        self.pair_prefix = pair_prefix
        self.hash_cache: WeakKeyDictionary[CLVMStorage, bytes32] = WeakKeyDictionary()
        self.cache_hits = 0

    def shatree_atom(self, atom: bytes) -> bytes32:
        s = sha256()
        s.update(self.atom_prefix)
        s.update(atom)
        return bytes32(s.digest())

    def shatree_pair(self, left_hash: bytes32, right_hash: bytes32) -> bytes32:
        s = sha256()
        s.update(self.pair_prefix)
        s.update(left_hash)
        s.update(right_hash)
        return bytes32(s.digest())

    def sha256_treehash(self, sexp: CLVMStorage) -> bytes32:
        def handle_sexp(sexp_stack, hash_stack, op_stack) -> None:
            sexp = sexp_stack.pop()
            r = getattr(sexp, "_cached_sha256_treehash", None)
            if r is None:
                r = self.hash_cache.get(sexp)
            if r is not None:
                self.cache_hits += 1
                hash_stack.append(r)
                return
            elif sexp.pair:
                p0, p1 = sexp.pair
                sexp_stack.append(p0)
                sexp_stack.append(p1)
                op_stack.append(handle_pair)
                op_stack.append(handle_sexp)
                op_stack.append(handle_sexp)
            else:
                r = shatree_atom(sexp.atom)
                hash_stack.append(r)
                sexp._cached_sha256_treehash = r
                self.hash_cache[sexp] = r

        def handle_pair(sexp_stack, hash_stack, op_stack) -> None:
            p0 = hash_stack.pop()
            p1 = hash_stack.pop()
            r = shatree_pair(p0, p1)
            hash_stack.append(r)
            sexp._cached_sha256_treehash = r
            self.hash_cache[sexp] = r

        sexp_stack = [sexp]
        op_stack = [handle_sexp]
        hash_stack: List[bytes32] = []
        while len(op_stack) > 0:
            op = op_stack.pop()
            op(sexp_stack, hash_stack, op_stack)
        return hash_stack[0]


CHIA_TREEHASHER = Treehasher(bytes.fromhex("01"), bytes.fromhex("02"))

sha256_treehash = CHIA_TREEHASHER.sha256_treehash
shatree_atom = CHIA_TREEHASHER.shatree_atom
shatree_pair = CHIA_TREEHASHER.shatree_pair
