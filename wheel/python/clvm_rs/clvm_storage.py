from typing import Optional, Tuple

# we support py3.7 which doesn't yet have typing.Protocol

try:
    from typing import Protocol, runtime_checkable
except ImportError:
    Protocol = object
    runtime_checkable = lambda arg: arg


@runtime_checkable
class CLVMStorage(Protocol):
    atom: Optional[bytes]

    @property
    def pair(self) -> Optional[Tuple["CLVMStorage", "CLVMStorage"]]:
        ...

    # optional fields used to speed implementations:

    # `_cached_sha256_treehash: Optional[bytes]` is used by `sha256_treehash`
    # `_cached_serialization:  bytes` is used by `sexp_to_byte_iterator` to speed up serialization


def is_clvm_storage(obj):
    return hasattr(obj, "atom") and hasattr(obj, "pair")
