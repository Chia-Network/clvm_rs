from typing import Any, Tuple, Union

AtomCastableType = Union[
    bytes,
    str,
    int,
    None,
]


CastableType = Union[
    AtomCastableType,
    list,
    Tuple["CastableType", "CastableType"],
]


def looks_like_clvm_object(o: Any) -> bool:
    d = dir(o)
    return "atom" in d and "pair" in d


def int_to_bytes(v):
    byte_count = (v.bit_length() + 8) >> 3
    if v == 0:
        return b""
    r = v.to_bytes(byte_count, "big", signed=True)
    # make sure the string returned is minimal
    # ie. no leading 00 or ff bytes that are unnecessary
    assert not (len(r) > 1 and r[0] == (0xFF if r[1] & 0x80 else 0))
    return r


NULL = b""


def to_atom_type(v: AtomCastableType) -> bytes:

    if isinstance(v, bytes):
        return v
    if isinstance(v, str):
        return v.encode()
    if isinstance(v, int):
        return int_to_bytes(v)
    if hasattr(v, "__bytes__"):
        return bytes(v)
    if v is None:
        return NULL

    raise ValueError("can't cast %s (%s) to bytes" % (type(v), v))


def to_clvm_object(
    v: CastableType,
    to_atom_f,
    to_pair_f,
):
    stack = [v]
    ops = [(0, None)]  # convert

    while len(ops) > 0:
        op, target = ops.pop()
        # convert value
        if op == 0:
            if looks_like_clvm_object(stack[-1]):
                obj = stack.pop()
                if obj.pair is None:
                    new_obj = to_atom_f(obj.atom)
                else:
                    new_obj = to_pair_f(obj.pair[0], obj.pair[1])
                stack.append(new_obj)
                continue
            v = stack.pop()
            if isinstance(v, tuple):
                if len(v) != 2:
                    raise ValueError("can't cast tuple of size %d" % len(v))
                left, right = v
                target = len(stack)
                ll_right = looks_like_clvm_object(right)
                ll_left = looks_like_clvm_object(left)
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
                    if not looks_like_clvm_object(_):
                        ops.append((0, None))  # convert
                continue
            stack.append(to_atom_f(to_atom_type(v)))
            continue

        if op == 1:  # prepend list
            stack[target] = to_pair_f(stack.pop(), stack[target])
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
