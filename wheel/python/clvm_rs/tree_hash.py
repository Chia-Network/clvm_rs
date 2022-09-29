"""
This is an implementation of `sha256_treehash`, used to calculate
puzzle hashes in clvm.

This implementation goes to great pains to be non-recursive so we don't
have to worry about blowing out the python stack.
"""

from hashlib import sha256

from clvm_rs import CLVMObject

bytes32 = bytes


ONE = bytes.fromhex("01")
TWO = bytes.fromhex("02")


def shatree_atom(atom: bytes) -> bytes32:
    s = sha256()
    s.update(ONE)
    s.update(atom)
    return bytes32(s.digest())


def shatree_pair(left_hash: bytes32, right_hash: bytes32) -> bytes32:
    s = sha256()
    s.update(TWO)
    s.update(left_hash)
    s.update(right_hash)
    return bytes32(s.digest())


def sha256_treehash(sexp: CLVMObject) -> bytes32:
    def handle_sexp(sexp_stack, op_stack) -> None:
        sexp = sexp_stack.pop()
        r = getattr(sexp, "_cached_sha256_treehash", None)
        if r is not None:
            sexp_stack.append(r)
        elif sexp.pair:
            p0, p1 = sexp.pair
            sexp_stack.append(p0)
            sexp_stack.append(p1)
            op_stack.append(handle_pair)
            op_stack.append(handle_sexp)
            op_stack.append(roll)
            op_stack.append(handle_sexp)
        else:
            r = shatree_atom(sexp.atom)
            sexp_stack.append(r)
            if hasattr(sexp, "_cached_sha256_treehash"):
                sexp._cached_sha256_treehash = r

    def handle_pair(sexp_stack, op_stack) -> None:
        p0 = sexp_stack.pop()
        p1 = sexp_stack.pop()
        r = shatree_pair(p0, p1)
        sexp_stack.append(r)
        if hasattr(sexp, "_cached_sha256_treehash"):
            sexp._cached_sha256_treehash = r

    def roll(sexp_stack, op_stack) -> None:
        p0 = sexp_stack.pop()
        p1 = sexp_stack.pop()
        sexp_stack.append(p0)
        sexp_stack.append(p1)

    sexp_stack = [sexp]
    op_stack = [handle_sexp]
    while len(op_stack) > 0:
        op = op_stack.pop()
        op(sexp_stack, op_stack)
    return bytes32(sexp_stack[0])
