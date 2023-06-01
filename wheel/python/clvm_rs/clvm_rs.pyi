from typing import List, Optional, Tuple

from .clvm_storage import CLVMStorage

def run_serialized_chia_program(
    program: bytes, environment: bytes, max_cost: int, flags: int
) -> Tuple[int, CLVMStorage]: ...
def deserialize_as_tree(
    blob: bytes, calculate_tree_hashes: bool
) -> Tuple[List[Tuple[int, int, int]], Optional[List[bytes]]]: ...
def serialized_length(blob: bytes) -> int: ...

NO_NEG_DIV: int
NO_UNKNOWN_OPS: int
LIMIT_HEAP: int
MEMPOOL_MODE: int

class LazyNode(CLVMStorage):
    atom: Optional[bytes]

    @property
    def pair(self) -> Optional[Tuple[CLVMStorage, CLVMStorage]]: ...
