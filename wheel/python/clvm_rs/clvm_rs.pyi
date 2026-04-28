from typing import List, Optional, Tuple

from .clvm_storage import CLVMStorage

def run_serialized_chia_program(
    program: bytes, environment: bytes, max_cost: int, flags: int
) -> Tuple[int, CLVMStorage]: ...
def deserialize_as_tree(
    blob: bytes, calculate_tree_hashes: bool
) -> Tuple[List[Tuple[int, int, int]], Optional[List[bytes]]]: ...
def serialized_length(blob: bytes) -> int: ...

# --- Deserialize functions ---
def deser_legacy(blob: bytes) -> "LazyNode": ...
def deser_backrefs(blob: bytes) -> "LazyNode": ...
def deser_2026(
    blob: bytes,
    *,
    max_atom_len: Optional[int] = None,
    max_input_bytes: Optional[int] = None,
) -> "LazyNode": ...
def deser_auto(
    blob: bytes,
    *,
    max_atom_len: Optional[int] = None,
    max_input_bytes: Optional[int] = None,
) -> "LazyNode": ...

# --- Serialize functions ---
def ser_legacy(node: "LazyNode") -> bytes: ...
def ser_backrefs(node: "LazyNode") -> bytes: ...
def ser_2026(
    node: "LazyNode",
    *,
    level: int = 1,
    prefixed: bool = True,
) -> bytes: ...

# --- Tree conversion ---
def clvm_tree_to_lazy_node(obj: CLVMStorage) -> "LazyNode": ...

NO_UNKNOWN_OPS: int
LIMIT_HEAP: int
MEMPOOL_MODE: int
ENABLE_SHA256_TREE: int
ENABLE_SECP_OPS: int
DISABLE_OP: int
CANONICAL_INTS: int

class LazyNode(CLVMStorage):
    atom: Optional[bytes]

    @property
    def pair(self) -> Optional[Tuple[CLVMStorage, CLVMStorage]]: ...
