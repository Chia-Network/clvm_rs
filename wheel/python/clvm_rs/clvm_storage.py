from typing import Optional, Protocol, Tuple, runtime_checkable


@runtime_checkable
class CLVMStorage(Protocol):
    atom: Optional[bytes]

    @property
    def pair(self) -> Optional[Tuple["CLVMStorage", "CLVMStorage"]]:
        ...

    # optional fields used to speed implementations:

    # `_cached_sha256_treehash: Optional[bytes]` is used by `sha256_treehash`
    # `_cached_serialization:  bytes` is used by `sexp_to_byte_iterator` to speed up serialization


@runtime_checkable
class CLVMStorageFactory(Protocol):
    @classmethod
    def new_atom(cls, v: bytes) -> "CLVMStorage":
        ...

    @classmethod
    def new_pair(cls, p1: "CLVMStorage", p2: "CLVMStorage") -> "CLVMStorage":
        ...
