"""Serialize and deserialize CLVM in legacy, backref, and 2026 formats.

All functions operate on bytes or LazyNode handles backed by Rust.

    from clvm_rs.serde import deserialize, serialize

    node = deserialize(blob)                    # auto-detects format
    out  = serialize(node, "2026")              # serde_2026 with magic prefix
    out  = serialize(node, "legacy")            # classic format
    out  = serialize(node, "2026", level=0)     # serde_2026, fast (no pair optimization)

Converting a Python CLVM tree to a Rust-backed LazyNode (with interning):

    from clvm_rs.serde import clvm_tree_to_lazy_node

    lazy = clvm_tree_to_lazy_node(program)
    blob = serialize(lazy, "2026")
"""

from .clvm_rs import (
    clvm_tree_to_lazy_node,
    deser_2026,
    deser_auto,
    deser_backrefs,
    deser_legacy,
    ser_2026,
    ser_backrefs,
    ser_legacy,
)

__all__ = [
    "serialize",
    "deserialize",
    "clvm_tree_to_lazy_node",
    "deser_legacy",
    "deser_backrefs",
    "deser_2026",
    "deser_auto",
    "ser_legacy",
    "ser_backrefs",
    "ser_2026",
]

_DESERIALIZERS = {
    "legacy": deser_legacy,
    "backrefs": deser_backrefs,
    "2026": deser_2026,
    "auto": deser_auto,
}

_SERIALIZERS = {
    "legacy": ser_legacy,
    "backrefs": ser_backrefs,
    "2026": ser_2026,
}


def deserialize(
    blob: bytes,
    fmt: str = "auto",
    *,
    max_atom_len: int | None = None,
    max_input_bytes: int | None = None,
):
    """Deserialize bytes into a LazyNode.

    Formats: "auto" (default), "legacy", "backrefs", "2026".
    "auto" handles all three formats by inspecting the magic prefix.

    Keyword-only limits (applied to "2026" and "auto" paths):
        max_atom_len:   largest single atom in bytes (default ~1 MB)
        max_input_bytes: total input budget in bytes  (default ~10 MB)
    """
    fn = _DESERIALIZERS.get(fmt)
    if fn is None:
        raise ValueError(f"unknown deserialize format {fmt!r}, expected one of {list(_DESERIALIZERS)}")
    if fmt in ("2026", "auto"):
        kwargs: dict = {}
        if max_atom_len is not None:
            kwargs["max_atom_len"] = max_atom_len
        if max_input_bytes is not None:
            kwargs["max_input_bytes"] = max_input_bytes
        return fn(blob, **kwargs)
    return fn(blob)


def serialize(node, fmt: str = "2026", *, level: int = 1, prefixed: bool = True) -> bytes:
    """Serialize a LazyNode to bytes.

    Formats: "2026" (default), "legacy", "backrefs".

    For "2026" format:
        level=0: left-first traversal (fast)
        level=1: pair-optimized DP traversal (smaller output, default)
        prefixed=True: prepend the magic prefix (default)
    """
    fn = _SERIALIZERS.get(fmt)
    if fn is None:
        raise ValueError(f"unknown serialize format {fmt!r}, expected one of {list(_SERIALIZERS)}")
    if fmt == "2026":
        return fn(node, level=level, prefixed=prefixed)
    return fn(node)
