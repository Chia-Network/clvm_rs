from typing import Any, Callable, List, Optional, SupportsBytes, Tuple, Union

from .clvm_storage import CLVMStorage

AtomCastableType = Union[
    bytes,
    str,
    int,
    SupportsBytes,
    None,
]


CastableType = Union[
    AtomCastableType,
    List["CastableType"],
    Tuple["CastableType", "CastableType"],
    CLVMStorage,
]


def int_from_bytes(blob):
    size = len(blob)
    if size == 0:
        return 0
    return int.from_bytes(blob, "big", signed=True)


def int_to_bytes(v) -> bytes:
    byte_count = (v.bit_length() + 8) >> 3
    if v == 0:
        return b""
    r = v.to_bytes(byte_count, "big", signed=True)
    # make sure the string returned is minimal
    # ie. no leading 00 or ff bytes that are unnecessary
    while len(r) > 1 and r[0] == (0xFF if r[1] & 0x80 else 0):
        r = r[1:]
    return r


NULL = b""


def to_atom_type(v: AtomCastableType) -> bytes:

    if isinstance(v, bytes):
        return v
    if isinstance(v, str):
        return v.encode()
    if isinstance(v, int):
        return int_to_bytes(v)
    if isinstance(v, (memoryview, SupportsBytes)):
        return bytes(v)
    if v is None:
        return NULL

    raise ValueError("can't cast %s (%s) to bytes" % (type(v), v))


def to_clvm_object(
    v: CastableType,
    to_atom_f: Callable[[bytes], CLVMStorage],
    to_pair_f: Callable[[CLVMStorage, CLVMStorage], CLVMStorage],
):
    stack: List[CastableType] = [v]
    ops: List[Tuple[int, Optional[CastableType]]] = [(0, None)]  # convert

    while len(ops) > 0:
        op, target = ops.pop()
        # convert value
        if op == 0:
            v = stack.pop()
            if isinstance(v, CLVMStorage):
                if v.pair is None:
                    atom = v.atom
                    assert atom is not None
                    new_obj = to_atom_f(to_atom_type(atom))
                else:
                    new_obj = to_pair_f(v.pair[0], v.pair[1])
                stack.append(new_obj)
                continue
            if isinstance(v, tuple):
                if len(v) != 2:
                    raise ValueError("can't cast tuple of size %d" % len(v))
                left, right = v
                target = len(stack)
                ll_right = isinstance(right, CLVMStorage)
                ll_left = isinstance(left, CLVMStorage)
                if ll_right and ll_left:
                    stack.append(to_pair_f(left, right))
                else:
                    ops.append((3, None))  # cons
                    stack.append(right)
                    ops.append((0, None))  # convert
                    ops.append((2, None))  # roll
                    stack.append(left)
                    ops.append((0, None))  # convert
                continue
            if isinstance(v, list):
                target = len(stack)
                stack.append(to_atom_f(NULL))
                for _ in v:
                    stack.append(_)
                    ops.append((1, target))  # prepend list
                    # we only need to convert if it's not already the right
                    # type
                    if not isinstance(_, CLVMStorage):
                        ops.append((0, None))  # convert
                continue
            stack.append(to_atom_f(to_atom_type(v)))
            continue

        if op == 1:  # prepend list
            left = stack.pop()
            assert isinstance(target, int)
            right = stack[target]
            stack[target] = to_pair_f(left, right)
            continue
        if op == 2:  # roll
            p1 = stack.pop()
            p2 = stack.pop()
            stack.append(p1)
            stack.append(p2)
            continue
        if op == 3:  # cons
            right = stack.pop()
            left = stack.pop()
            obj = to_pair_f(left, right)
            stack.append(obj)
            continue
    # there's exactly one item left at this point
    assert len(stack) == 1

    # stack[0] implements the clvm object protocol
    return stack[0]
