from typing import List, Tuple

from .tree_hash import shatree_atom, shatree_pair


MAX_SINGLE_BYTE = 0x7F
CONS_BOX_MARKER = 0xFF


# ATOM: serialize_offset, serialize_end, atom_offset
# PAIR: serialize_offset, serialize_end, right_index


def deserialize_as_tuples(
    blob: bytes, cursor: int = 0
) -> List[Tuple[int, int, int, bytes]]:
    def save_cursor(index, blob, cursor, obj_list, op_stack):
        assert blob[obj_list[index][0]] == 0xFF
        left_hash = obj_list[index + 1][3]
        right_hash = obj_list[obj_list[index][2]][3]
        my_hash = shatree_pair(left_hash, right_hash)
        obj_list[index] = (
            obj_list[index][0],
            cursor,
            obj_list[index][2],
            my_hash,
        )
        return cursor

    def save_index(index, blob, cursor, obj_list, op_stack):
        obj_list[index][2] = len(obj_list)
        return cursor

    def parse_obj(blob, cursor, obj_list, op_stack):
        if cursor >= len(blob):
            raise ValueError("bad encoding")

        if blob[cursor] == CONS_BOX_MARKER:
            index = len(obj_list)
            obj_list.append([cursor, None, None])
            op_stack.append(lambda *args: save_cursor(index, *args))
            op_stack.append(parse_obj)
            op_stack.append(lambda *args: save_index(index, *args))
            op_stack.append(parse_obj)
            return cursor + 1
        atom_offset, new_cursor = _atom_size_from_cursor(blob, cursor)
        my_hash = shatree_atom(blob[cursor + atom_offset : new_cursor])
        obj_list.append((cursor, new_cursor, atom_offset, my_hash))
        return new_cursor

    obj_list = []
    op_stack = [parse_obj]
    while op_stack:
        f = op_stack.pop()
        cursor = f(blob, cursor, obj_list, op_stack)
    return obj_list


def _atom_size_from_cursor(blob, cursor) -> Tuple[int, int]:
    # return `(size_of_prefix, cursor)`
    b = blob[cursor]
    if b == 0x80:
        return 1, cursor + 1
    if b <= MAX_SINGLE_BYTE:
        return 0, cursor + 1
    bit_count = 0
    bit_mask = 0x80
    while b & bit_mask:
        bit_count += 1
        b &= 0xFF ^ bit_mask
        bit_mask >>= 1
    size_blob = bytes([b])
    if bit_count > 1:
        size_blob += blob[cursor + 1 : cursor + bit_count]
    size = int.from_bytes(size_blob, "big")
    new_cursor = cursor + size + bit_count
    if new_cursor > len(blob):
        raise ValueError("end of stream")
    return bit_count, new_cursor
