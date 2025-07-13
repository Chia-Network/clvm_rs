from typing import Callable, List, Optional, Tuple

from .tree_hash import shatree_atom, shatree_pair

deserialize_as_tree: Optional[
    Callable[[bytes, bool], Tuple[List[Tuple[int, int, int]], Optional[List[bytes]]]]
]

try:
    from clvm_rs.clvm_rs import deserialize_as_tree
except ImportError:
    deserialize_as_tree = None


MAX_SINGLE_BYTE = 0x7F
CONS_BOX_MARKER = 0xFF


# ATOM: serialize_offset, serialize_end, atom_offset
# PAIR: serialize_offset, serialize_end, right_index

Triple = Tuple[int, int, int]
DeserOp = Callable[[bytes, int, List[Triple], List], int]


def deserialize_as_tuples(
    blob: bytes, cursor: int, calculate_tree_hash: bool
) -> Tuple[List[Tuple[int, int, int]], Optional[List[bytes]]]:
    if deserialize_as_tree:
        try:
            tree, hashes = deserialize_as_tree(blob, calculate_tree_hash)
        except OSError as ex:
            raise ValueError(ex)
        return tree, hashes

    def save_cursor(
        index: int,
        blob: bytes,
        cursor: int,
        obj_list: List[Triple],
        op_stack: List[DeserOp],
    ) -> int:
        blob_index = obj_list[index][0]
        assert blob[blob_index] == 0xFF
        v0 = obj_list[index][0]
        v2 = obj_list[index][2]
        obj_list[index] = (v0, cursor, v2)
        if calculate_tree_hash:
            left_hash = tree_hash_list[index + 1]
            hash_index = obj_list[index][2]
            right_hash = tree_hash_list[hash_index]
            tree_hash_list[index] = shatree_pair(left_hash, right_hash)
        return cursor

    def save_index(
        index: int,
        blob: bytes,
        cursor: int,
        obj_list: List[Triple],
        op_stack: List[DeserOp],
    ) -> int:
        e = obj_list[index]
        obj_list[index] = (e[0], e[1], len(obj_list))
        return cursor

    def parse_obj(
        blob: bytes, cursor: int, obj_list: List[Triple], op_stack: List[DeserOp]
    ) -> int:
        if cursor >= len(blob):
            raise ValueError("bad encoding")

        if blob[cursor] == CONS_BOX_MARKER:
            index = len(obj_list)
            obj_list.append((cursor, 0, 0))
            op_stack.append(lambda *args: save_cursor(index, *args))
            op_stack.append(parse_obj)
            op_stack.append(lambda *args: save_index(index, *args))
            op_stack.append(parse_obj)
            if calculate_tree_hash:
                tree_hash_list.append(b"")
            return cursor + 1
        atom_offset, new_cursor = _atom_size_from_cursor(blob, cursor)
        my_hash = None
        if calculate_tree_hash:
            my_hash = shatree_atom(blob[cursor + atom_offset:new_cursor])
            tree_hash_list.append(my_hash)
        obj_list.append((cursor, new_cursor, atom_offset))
        return new_cursor

    obj_list: List[Triple] = []
    tree_hash_list: List[bytes] = []
    op_stack: List[DeserOp] = [parse_obj]
    while op_stack:
        f = op_stack.pop()
        cursor = f(blob, cursor, obj_list, op_stack)
    return obj_list, tree_hash_list if calculate_tree_hash else None


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
        size_blob += blob[cursor + 1:cursor + bit_count]
    size = int.from_bytes(size_blob, "big")
    new_cursor = cursor + size + bit_count
    if new_cursor > len(blob):
        raise ValueError("end of stream")
    return bit_count, new_cursor
