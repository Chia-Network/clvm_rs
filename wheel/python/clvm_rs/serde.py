"""Serialize and deserialize CLVM in legacy, backref, and 2026 formats.

All functions operate on bytes or LazyNode handles backed by Rust.

    from clvm_rs.serde import serialize, deserialize, Format

    node = deserialize(blob, Format.BACKREFS)
    out  = serialize(node, Format.SER_2026)
"""

from enum import Enum

from .clvm_rs import (
    deser_2026,
    deser_backrefs,
    deser_legacy,
    ser_2026,
    ser_backrefs,
    ser_legacy,
)

__all__ = [
    "Format",
    "serialize",
    "deserialize",
    "deser_legacy",
    "deser_backrefs",
    "deser_2026",
    "ser_legacy",
    "ser_backrefs",
    "ser_2026",
]


class Format(Enum):
    LEGACY = "legacy"
    BACKREFS = "backrefs"
    SER_2026 = "2026"


_DESERIALIZERS = {
    Format.LEGACY: deser_legacy,
    Format.BACKREFS: deser_backrefs,
    Format.SER_2026: deser_2026,
}

_SERIALIZERS = {
    Format.LEGACY: ser_legacy,
    Format.BACKREFS: ser_backrefs,
    Format.SER_2026: ser_2026,
}


def deserialize(blob: bytes, fmt: Format = Format.LEGACY):
    """Deserialize bytes into a LazyNode."""
    return _DESERIALIZERS[fmt](blob)


def serialize(node, fmt: Format = Format.LEGACY) -> bytes:
    """Serialize a LazyNode to bytes."""
    return _SERIALIZERS[fmt](node)
