"""Serialize and deserialize CLVM in legacy, backref, and 2026 formats.

All functions operate on bytes or LazyNode handles backed by Rust.

    from clvm_rs.serde import serialize, deserialize, Format

    node = deserialize(blob, Format.BACKREFS)
    out  = serialize(node, Format.SER_2026)

Auto-detecting deserialization (recommended for new code):

    node = deserialize(blob, Format.AUTO)

Serializing with the serde_2026 magic prefix (for use with Format.AUTO):

    out = serialize(node, Format.SER_2026_PREFIXED)
"""

from enum import Enum

from .clvm_rs import (
    deser_2026,
    deser_auto,
    deser_backrefs,
    deser_legacy,
    deser_legacy_interned,
    intern,
    ser_2026,
    ser_2026_prefixed,
    ser_backrefs,
    ser_legacy,
)

__all__ = [
    "Format",
    "serialize",
    "deserialize",
    "intern",
    "deser_legacy",
    "deser_backrefs",
    "deser_legacy_interned",
    "deser_2026",
    "deser_auto",
    "ser_legacy",
    "ser_backrefs",
    "ser_2026",
    "ser_2026_prefixed",
]


class Format(Enum):
    LEGACY = "legacy"
    BACKREFS = "backrefs"
    SER_2026 = "2026"
    # Deserialize only: auto-detect format from magic prefix.
    AUTO = "auto"
    # Serialize only: serde_2026 with the fd ff 32 30 32 36 magic prefix prepended.
    SER_2026_PREFIXED = "2026_prefixed"


_DESERIALIZERS = {
    Format.LEGACY: deser_legacy,
    Format.BACKREFS: deser_backrefs,
    Format.SER_2026: deser_2026,
    Format.AUTO: deser_auto,
}

_SERIALIZERS = {
    Format.LEGACY: ser_legacy,
    Format.BACKREFS: ser_backrefs,
    Format.SER_2026: ser_2026,
    Format.SER_2026_PREFIXED: ser_2026_prefixed,
}


def deserialize(
    blob: bytes,
    fmt: Format = Format.AUTO,
    *,
    max_atom_len: int | None = None,
    max_input_bytes: int | None = None,
):
    """Deserialize bytes into a LazyNode.

    Defaults to Format.AUTO which handles classic, backrefs, and serde_2026.

    Keyword-only limits (only applied to serde_2026 / AUTO paths):
        max_atom_len:   largest single atom in bytes (default ~1 MB)
        max_input_bytes: total input budget in bytes  (default ~10 MB)
    """
    fn = _DESERIALIZERS.get(fmt)
    if fn is None:
        raise ValueError(f"Format {fmt!r} cannot be used for deserialization")
    if fmt in (Format.SER_2026, Format.AUTO):
        kwargs: dict = {}
        if max_atom_len is not None:
            kwargs["max_atom_len"] = max_atom_len
        if max_input_bytes is not None:
            kwargs["max_input_bytes"] = max_input_bytes
        return fn(blob, **kwargs)
    return fn(blob)


def serialize(node, fmt: Format = Format.LEGACY) -> bytes:
    """Serialize a LazyNode to bytes."""
    fn = _SERIALIZERS.get(fmt)
    if fn is None:
        raise ValueError(f"Format {fmt!r} cannot be used for serialization")
    return fn(node)
