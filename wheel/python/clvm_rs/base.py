from typing import Optional, Protocol, Tuple


class CLVMObject(Protocol):
    atom: Optional[bytes]
    pair: Optional[Tuple["CLVMObjectStore", "CLVMObjectStore"]]

    @classmethod
    def new_atom(cls, v: bytes) -> "CLVMObjectStore":
        raise NotImplementedError()

    @classmethod
    def new_pair(cls, p1, p2) -> "CLVMObjectStore":
        raise NotImplementedError()
