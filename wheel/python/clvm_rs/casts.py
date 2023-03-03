"""
Some utilities to cast python types to and from clvm.
"""

from typing import Callable, List, SupportsBytes, Tuple, Union, cast

from .clvm_storage import CLVMStorage, is_clvm_storage

AtomCastableType = Union[
    bytes,
    str,
    int,
    SupportsBytes,
]


# as of January 2023, mypy does not like this recursive definition

CastableType = Union[
    AtomCastableType,
    List["CastableType"],
    Tuple["CastableType", "CastableType"],
    CLVMStorage,
]


NULL_BLOB = b""


def int_from_bytes(blob):
    """
    Convert a bytes blob encoded as a clvm int to a python int.
    """
    size = len(blob)
    if size == 0:
        return 0
    return int.from_bytes(blob, "big", signed=True)


def int_to_bytes(v) -> bytes:
    """
    Convert a python int to a blob that encodes as this integer in clvm.
    """
    byte_count = (v.bit_length() + 8) >> 3
    if v == 0:
        return b""
    r = v.to_bytes(byte_count, "big", signed=True)
    # make sure the string returned is minimal
    # ie. no leading 00 or ff bytes that are unnecessary
    while len(r) > 1 and r[0] == (0xFF if r[1] & 0x80 else 0):
        r = r[1:]
    return r


def to_atom_type(v: AtomCastableType) -> bytes:
    """
    Convert an `AtomCastableType` to `bytes`. This for use with the
    convenience function `Program.to`.
    """
    if isinstance(v, bytes):
        return v
    if isinstance(v, str):
        return v.encode()
    if isinstance(v, int):
        return int_to_bytes(v)
    if isinstance(v, (memoryview, SupportsBytes)):
        return bytes(v)

    raise ValueError("can't cast %s (%s) to bytes" % (type(v), v))


def to_clvm_object(
    castable: CastableType,
    to_atom_f: Callable[[bytes], CLVMStorage],
    to_pair_f: Callable[[CLVMStorage, CLVMStorage], CLVMStorage],
):
    """
    Convert a python object to clvm object.

    This works on nested tuples and lists of potentially unlimited depth.
    It is non-recursive, so nesting depth is not limited by the call stack.
    """
    to_convert: List[CastableType] = [castable]
    did_convert: List[CLVMStorage] = []
    ops: List[int] = [0]

    # operations:
    #  0: pop `to_convert` and convert if possible, storing result on `did_convert`,
    #     or subdivide task, pushing multiple things on `to_convert` (and new ops)
    #  1: pop & cons two items from `did_convert`, pushing result to `did_convert`
    #  2: same as 1 but cons in opposite order. Necessary for converting lists

    while len(ops) > 0:
        op = ops.pop()
        # convert value
        if op == 0:
            v = to_convert.pop()
            if is_clvm_storage(v):
                v = cast(CLVMStorage, v)
                did_convert.append(v)
                continue
            if isinstance(v, tuple):
                if len(v) != 2:
                    raise ValueError("can't cast tuple of size %d" % len(v))
                left, right = v
                ll_right = is_clvm_storage(right)
                ll_left = is_clvm_storage(left)
                if ll_right and ll_left:
                    left = cast(CLVMStorage, left)
                    right = cast(CLVMStorage, right)
                    did_convert.append(to_pair_f(left, right))
                else:
                    ops.append(1)  # cons
                    to_convert.append(left)
                    ops.append(0)  # convert
                    to_convert.append(right)
                    ops.append(0)  # convert
                continue
            if isinstance(v, list):
                for _ in v:
                    ops.append(2)  # rcons

                # add and convert the null terminator
                to_convert.append(to_atom_f(NULL_BLOB))
                ops.append(0)  # convert

                for _ in reversed(v):
                    to_convert.append(_)
                    ops.append(0)  # convert
                continue
            v = cast(AtomCastableType, v)
            did_convert.append(to_atom_f(to_atom_type(v)))
            continue
        if op == 1:  # cons
            left = did_convert.pop()
            right = did_convert.pop()
            obj = to_pair_f(left, right)
            did_convert.append(obj)
            continue
        if op == 2:  # rcons
            right = did_convert.pop()
            left = did_convert.pop()
            obj = to_pair_f(left, right)
            did_convert.append(obj)
            continue

    # there's exactly one item left at this point
    assert len(did_convert) == 1
    return did_convert[0]
