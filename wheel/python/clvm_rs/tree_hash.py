"""
This is an implementation of `sha256_treehash`, used to calculate
puzzle hashes in clvm.

This implementation goes to great pains to be non-recursive so we don't
have to worry about blowing out the python stack.
"""

from hashlib import sha256
from typing import Callable, List, Tuple, cast

from .clvm_storage import CLVMStorage


OP_STACK_F = Callable[[List[CLVMStorage], List[bytes], List["OP_STACK_F"]], None]


class Treehasher:
    """
    `Treehasher` performs the standard sha256tree hashing in a non-recursive
    way so that extremely large objects don't blow out the python stack.

    We also force a `_cached_sha256_treehash` into the hashed sub-objects
    whenever possible so that taking the hash of the same sub-tree is
    more efficient in future.
    """

    atom_prefix: bytes
    pair_prefix: bytes
    cache_hits: int

    def __init__(self, atom_prefix: bytes, pair_prefix: bytes):
        self.atom_prefix = atom_prefix
        self.pair_prefix = pair_prefix
        self.cache_hits = 0

    def shatree_atom(self, atom: bytes) -> bytes:
        s = sha256()
        s.update(self.atom_prefix)
        s.update(atom)
        return s.digest()

    def shatree_pair(self, left_hash: bytes, right_hash: bytes) -> bytes:
        s = sha256()
        s.update(self.pair_prefix)
        s.update(left_hash)
        s.update(right_hash)
        return s.digest()

    def sha256_treehash(self, clvm_storage: CLVMStorage) -> bytes:
        def handle_obj(
            obj_stack: List[CLVMStorage],
            hash_stack: List[bytes],
            op_stack: List[OP_STACK_F],
        ) -> None:
            obj = obj_stack.pop()
            r = getattr(obj, "_cached_sha256_treehash", None)
            if r is not None:
                self.cache_hits += 1
                hash_stack.append(r)
                return
            elif obj.atom is not None:
                r = shatree_atom(obj.atom)
                hash_stack.append(r)
                try:
                    setattr(obj, "_cached_sha256_treehash", r)
                except AttributeError:
                    pass
            else:
                pair = cast(Tuple[CLVMStorage, CLVMStorage], obj.pair)
                p0, p1 = pair
                obj_stack.append(obj)
                obj_stack.append(p0)
                obj_stack.append(p1)
                op_stack.append(handle_pair)
                op_stack.append(handle_obj)
                op_stack.append(handle_obj)

        def handle_pair(
            obj_stack: List[CLVMStorage],
            hash_stack: List[bytes],
            op_stack: List[OP_STACK_F],
        ) -> None:
            p0 = hash_stack.pop()
            p1 = hash_stack.pop()
            r = shatree_pair(p0, p1)
            hash_stack.append(r)
            obj = obj_stack.pop()
            try:
                setattr(obj, "_cached_sha256_treehash", r)
            except AttributeError:
                pass

        obj_stack: List[CLVMStorage] = [clvm_storage]
        op_stack: List[OP_STACK_F] = [handle_obj]
        hash_stack: List[bytes] = []
        while len(op_stack) > 0:
            op: OP_STACK_F = op_stack.pop()
            op(obj_stack, hash_stack, op_stack)
        return hash_stack[0]


CHIA_TREE_HASH_ATOM_PREFIX = bytes.fromhex("01")
CHIA_TREE_HASH_PAIR_PREFIX = bytes.fromhex("02")
CHIA_TREEHASHER = Treehasher(CHIA_TREE_HASH_ATOM_PREFIX, CHIA_TREE_HASH_PAIR_PREFIX)

sha256_treehash = CHIA_TREEHASHER.sha256_treehash
shatree_atom = CHIA_TREEHASHER.shatree_atom
shatree_pair = CHIA_TREEHASHER.shatree_pair
