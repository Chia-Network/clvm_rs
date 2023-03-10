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


CastableType = Union[
    AtomCastableType,
    List["CastableType"],
    Tuple["CastableType", "CastableType"],
    CLVMStorage,
]


NULL_BLOB = b""


def generate_working_memview_or_bytes():
    # in python3.7 you can't go 'isinstance(b"", SupportsBytes)'

    # this one works in py38+
    def memview_or_bytes_py38_or_later(o):
        return isinstance(o, (memoryview, SupportsBytes))

    # this one works in py37
    def memview_or_bytes_py37(o):
        return getattr(o, "__bytes__", None) is not None

    try:
        memview_or_bytes_py38_or_later(b"")
        return memview_or_bytes_py38_or_later
    except TypeError:
        pass
    return memview_or_bytes_py37


memview_or_bytes = generate_working_memview_or_bytes()


def int_from_bytes(blob: bytes) -> int:
    """
    Convert a bytes blob encoded as a clvm int to a python int.
    """
    size = len(blob)
    if size == 0:
        return 0
    return int.from_bytes(blob, "big", signed=True)


def int_to_bytes(v: int) -> bytes:
    """
    Convert a python int to a blob that encodes as this integer in clvm.
    """
    if v == 0:
        return b""
    byte_count = (v.bit_length() + 8) // 8
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
    if memview_or_bytes(v):
        return bytes(v)

    raise ValueError("can't cast %s (%s) to bytes" % (type(v), v))


def to_clvm_object(
    castable: CastableType,
    to_atom_f: Callable[[bytes], CLVMStorage],
    to_pair_f: Callable[[CLVMStorage, CLVMStorage], CLVMStorage],
) -> CLVMStorage:
    """
    Convert a python object to clvm object.

    This works on nested tuples and lists of potentially unlimited depth.
    It is non-recursive, so nesting depth is not limited by the call stack.
    But the entire hiearachy must be traversed, so execution time is
    proportional to hierarchy depth, and thus potentially unbounded.

    So don't use on untrusted input. Also, the case where a list that transitively
    contains itself (eg `t = []; t.append(t)`) in a loop is not detected, and
    this function will never return because it acts like a hiearachy with
    infinite depth.
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
