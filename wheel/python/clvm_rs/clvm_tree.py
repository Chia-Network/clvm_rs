from clvm_rs import deserialize_as_triples


from typing import List, Optional, Tuple


class CLVMTree:
    """
    This object conforms with the `CLVMObject` protocol. It's optimized for
    deserialization, and keeps a reference to the serialized blob and to a
    list of triples of integers, each of which corresponds to a subtree.

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
    serialized data and the list of triples, no memory is released until all
    objects in the tree are garbage-collected. This happens pretty naturally
    in well-behaved python code.
    """

    @classmethod
    def from_bytes(cls, blob: bytes) -> "CLVMTree":
        return cls(memoryview(blob), deserialize_as_triples(blob), 0)

    def __init__(
        self, blob: bytes, int_triples: List[Tuple[int, int, int]], index: int
    ):
        self.blob = blob
        self.int_triples = int_triples
        self.index = index

    @property
    def atom(self) -> Optional[bytes]:
        start, end, atom_offset = self.int_triples[self.index]
        # if `self.blob[start]` is 0xff, it's a pair
        if self.blob[start] == 0xFF:
            return None
        return bytes(self.blob[start + atom_offset:end])

    @property
    def pair(self) -> Optional[Tuple["CLVMTree", "CLVMTree"]]:
        start, end, right_index = self.int_triples[self.index]
        # if `self.blob[start]` is 0xff, it's a pair
        if self.blob[start] != 0xFF:
            return None
        left = self.__class__(self.blob, self.int_triples, self.index + 1)
        right = self.__class__(self.blob, self.int_triples, right_index)
        return (left, right)

    @property
    def _cached_serialization(self) -> bytes:
        start, end, _ = self.int_triples[self.index]
        return self.blob[start:end]

    def __bytes__(self) -> bytes:
        return bytes(self._cached_serialization)

    def __str__(self) -> str:
        return bytes(self).hex()
        a = self.atom
        if a is not None:
            return a.hex()
        return f"({self.index+1}, {self.int_triples[self.index][2]})"

    def __repr__(self) -> str:
        return f"<{self.__class__.__name__}: {self}>"
