"""
Serialize clvm.

# decoding:
# read a byte
# if it's 0xfe, it's nil (which might be same as 0)
# if it's 0xff, it's a cons box. Read two items, build cons
# otherwise, number of leading set bits is length in bytes to read size
# 0-0x7f are literal one byte values
# leading bits is the count of bytes to read of size
# 0x80-0xbf is a size of one byte (`and` of first byte with 0x3f for size)
# 0xc0-0xdf is a size of two bytes (`and` of first byte with 0x1f)
# 0xe0-0xef is 3 bytes (`and` of first byte with 0xf)
# 0xf0-0xf7 is 4 bytes (`and` of first byte with 0x7)
# 0xf7-0xfb is 5 bytes (`and` of first byte with 0x3)
"""

from typing import BinaryIO, Callable, Iterator, List

from .clvm_storage import CLVMStorage


MAX_SINGLE_BYTE = 0x7F
CONS_BOX_MARKER = 0xFF


def sexp_to_byte_iterator(sexp: CLVMStorage) -> Iterator[bytes]:
    """
    Yields bytes that serialize the given clvm object. Non-recursive
    """
    todo_stack = [sexp]
    while todo_stack:
        sexp = todo_stack.pop()
        r = getattr(sexp, "_cached_serialization", None)
        if r is not None:
            yield r
            continue
        pair = sexp.pair
        if pair:
            yield bytes([CONS_BOX_MARKER])
            todo_stack.append(pair[1])
            todo_stack.append(pair[0])
        else:
            atom = sexp.atom
            assert atom is not None
            yield from atom_to_byte_iterator(atom)


def size_blob_for_blob(blob: bytes) -> bytes:
    size = len(blob)
    if size < 0x40:
        return bytes([0x80 | size])
    if size < 0x2000:
        return bytes([0xC0 | (size >> 8), (size >> 0) & 0xFF])
    if size < 0x100000:
        return bytes([0xE0 | (size >> 16), (size >> 8) & 0xFF, (size >> 0) & 0xFF])
    if size < 0x8000000:
        return bytes(
            [
                0xF0 | (size >> 24),
                (size >> 16) & 0xFF,
                (size >> 8) & 0xFF,
                (size >> 0) & 0xFF,
            ]
        )
    if size < 0x400000000:
        return bytes(
            [
                0xF8 | (size >> 32),
                (size >> 24) & 0xFF,
                (size >> 16) & 0xFF,
                (size >> 8) & 0xFF,
                (size >> 0) & 0xFF,
            ]
        )
    raise ValueError("blob too long %r" % blob)


def atom_to_byte_iterator(as_atom: bytes) -> Iterator[bytes]:
    """
    Yield the serialization for a given blob (as a clvm atom).
    """
    size = len(as_atom)
    if size == 0:
        yield b"\x80"
        return
    if size == 1:
        if as_atom[0] <= MAX_SINGLE_BYTE:
            yield as_atom
            return
    yield size_blob_for_blob(as_atom)
    yield as_atom


def sexp_to_stream(sexp: CLVMStorage, f: BinaryIO) -> None:
    """
    Serialize to a file.
    """
    for b in sexp_to_byte_iterator(sexp):
        f.write(b)


def sexp_to_bytes(sexp: CLVMStorage) -> bytes:
    b = bytearray()
    for _ in sexp_to_byte_iterator(sexp):
        b.extend(_)
    return bytes(b)


NEW_PAIR_F = Callable[[CLVMStorage, CLVMStorage], CLVMStorage]
NEW_ATOM_F = Callable[[bytes], CLVMStorage]
OP_STACK_F = Callable[
    [List["OP_STACK_F"], List[CLVMStorage], BinaryIO, NEW_PAIR_F, NEW_ATOM_F], None
]


def _op_read_sexp(
    op_stack: List[OP_STACK_F],
    val_stack: List[CLVMStorage],
    f: BinaryIO,
    new_pair_f: NEW_PAIR_F,
    new_atom_f: NEW_ATOM_F,
):
    blob = f.read(1)
    if len(blob) == 0:
        raise ValueError("bad encoding")
    b = blob[0]
    if b == CONS_BOX_MARKER:
        op_stack.append(_op_cons)
        op_stack.append(_op_read_sexp)
        op_stack.append(_op_read_sexp)
        return
    val_stack.append(_atom_from_stream(f, b, new_atom_f))


def _op_cons(
    op_stack: List[OP_STACK_F],
    val_stack: List[CLVMStorage],
    f: BinaryIO,
    new_pair_f: NEW_PAIR_F,
    new_atom_f: NEW_ATOM_F,
):
    right = val_stack.pop()
    left = val_stack.pop()
    val_stack.append(new_pair_f(left, right))


def sexp_from_stream(
    f: BinaryIO, new_pair_f: NEW_PAIR_F, new_atom_f: NEW_ATOM_F
) -> CLVMStorage:
    op_stack: List[OP_STACK_F] = [_op_read_sexp]
    val_stack: List[CLVMStorage] = []

    while op_stack:
        func = op_stack.pop()
        func(op_stack, val_stack, f, new_pair_f, new_atom_f)
    return val_stack.pop()


def _atom_from_stream(f: BinaryIO, b: int, new_atom_f: NEW_ATOM_F) -> CLVMStorage:
    if b == 0x80:
        return new_atom_f(b"")
    if b <= MAX_SINGLE_BYTE:
        return new_atom_f(bytes([b]))
    bit_count = 0
    bit_mask = 0x80
    while b & bit_mask:
        bit_count += 1
        b &= 0xFF ^ bit_mask
        bit_mask >>= 1
    size_blob = bytes([b])
    if bit_count > 1:
        blob = f.read(bit_count - 1)
        if len(blob) != bit_count - 1:
            raise ValueError("bad encoding")
        size_blob += blob
    size = int.from_bytes(size_blob, "big")
    if size >= 0x400000000:
        raise ValueError("blob too large")
    blob = f.read(size)
    if len(blob) != size:
        raise ValueError("bad encoding")
    return new_atom_f(blob)
