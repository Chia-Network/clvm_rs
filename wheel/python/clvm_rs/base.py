from typing import Optional, Protocol, Tuple


class CLVMObject(Protocol):
    atom: Optional[bytes]
    pair: Optional[Tuple["CLVMObject", "CLVMObject"]]

    @classmethod
    def new_atom(cls, v: bytes) -> "CLVMObject":
        raise NotImplementedError()

    @classmethod
    def new_pair(cls, p1, p2) -> "CLVMObject":
        raise NotImplementedError()
