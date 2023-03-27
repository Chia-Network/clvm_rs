from .clvm_storage import CLVMStorage
from .de import deserialize_as_tuples

from typing import List, Optional, Tuple, Union


class CLVMTree(CLVMStorage):
    """
    This object conforms with the `CLVMStorage` protocol. It's optimized for
    deserialization, and keeps a reference to the serialized blob and to a
    list of tuples of integers, each of which corresponds to a subtree.

    It turns out every atom serialized to a blob contains a substring that
    exactly matches that atom, so it ends up being not very wasteful to
    simply use the blob for atom storage (especially if it's a `memoryview`,
    from which you can take substrings without copying). Additionally, the
    serialization for every object in the tree exactly corresponds to a
    substring in the blob, so by clever caching we can very quickly generate
    serializations for any subtree.

    The deserializer iterates through the blob and caches a triple of
    integers for each subtree: the first two integers represent the
    `(start_offset, end_offset)` within the blob that corresponds to the
    serialization of that object. You can check the contents of
    `blob[start_offset]` to determine if the object is a pair (in which case
    that byte is 0xff) or an atom (anything else). For a pair, the third
    number corresponds to the index of the array that is the "rest" of the
    pair (the "first" is always this object's index plus one, so we don't
    need to save that); for an atom, the third number corresponds to an
    offset of where the atom's binary data is relative to
    `blob[start_offset]` (so the atom data is at `blob[triple[0] +
    triple[2]:triple[1]]`)

    Since each `CLVMTree` subtree keeps a reference to the original
    serialized data and the list of tuples, no memory is released until all
    objects in the tree are garbage-collected. This happens pretty naturally
    in well-behaved python code.
    """

    # cached value of lazily-created child `CLVMStorage` objects
    # created on call to `.pair`
    _pair: Optional[Tuple["CLVMStorage", "CLVMStorage"]]

    # the serialized form, which contains the template for all the atoms
    blob: Union[memoryview, bytes]

    # the memoized boundary list for all clvm objects in the tree
    # this list is shared by *all* instances of sub-objects of the
    # same tree
    # [int, int, int] => start_offset, end_offset, atom_start_or_pair_rest
    #   deserialization of this object is blob[start_offset, end_offset]
    # We look at blob[start_offset] and if 0xff, this is a pair, otherwise atom
    # if atom, atom is blob[atom_start_or_pair_rest:end_offset]
    # if pair, children have `index` start_offset+1 and atom_start_or_pair_rest
    int_tuples: List[Tuple[int, int, int]]

    # which of the clvm objects in the tree this instance represents
    index: int  # index into `int_tuples`

    # the sha256 tree hashes that are optionally created on deserialization
    tree_hashes: Optional[List[bytes]]

    @classmethod
    def from_bytes(cls, blob: bytes, calculate_tree_hash: bool = True) -> "CLVMTree":
        int_tuples, tree_hashes = deserialize_as_tuples(
            blob, 0, calculate_tree_hash=calculate_tree_hash
        )
        return cls(memoryview(blob), int_tuples, tree_hashes, 0)

    def __init__(
        self,
        blob: Union[memoryview, bytes],
        int_tuples: List[Tuple[int, int, int]],
        tree_hashes: Optional[List[bytes]],
        index: int,
    ):
        self.blob = blob
        self.int_tuples = int_tuples
        self.tree_hashes = tree_hashes
        self.index = index
        if self.tree_hashes:
            self._cached_sha256_treehash = self.tree_hashes[index]
        start, end, atom_offset = self.int_tuples[self.index]
        if self.blob[start] == 0xFF:
            self.atom = None
        else:
            self.atom = bytes(self.blob[start + atom_offset:end])
            self._pair = None

    @property
    def pair(self) -> Optional[Tuple["CLVMStorage", "CLVMStorage"]]:
        if not hasattr(self, "_pair"):
            tuples, tree_hashes = self.int_tuples, self.tree_hashes
            start, end, right_index = tuples[self.index]
            # if `self.blob[start]` is 0xff, it's a pair
            assert self.blob[start] == 0xFF
            left = self.__class__(self.blob, tuples, tree_hashes, self.index + 1)
            right = self.__class__(self.blob, tuples, tree_hashes, right_index)
            self._pair = (left, right)
        return self._pair

    @property
    def _cached_serialization(self) -> bytes:
        start, end, _ = self.int_tuples[self.index]
        return self.blob[start:end]

    def __bytes__(self) -> bytes:
        return bytes(self._cached_serialization)

    def __str__(self) -> str:
        return bytes(self).hex()

    def __repr__(self) -> str:
        return f"<{self.__class__.__name__}: {self}>"
