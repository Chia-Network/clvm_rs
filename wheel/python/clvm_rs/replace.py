from __future__ import annotations
from typing import Dict

from .casts import CastableType
from .clvm_storage import CLVMStorage


def replace(program: CLVMStorage, **kwargs: CastableType) -> CastableType:
    # if `kwargs == {}` then `return program` unchanged
    if len(kwargs) == 0:
        return program

    if "" in kwargs:
        if len(kwargs) > 1:
            raise ValueError("conflicting paths")
        return kwargs[""]

    # we've confirmed that no `kwargs` is the empty string.
    # Now split `kwargs` into two groups: those
    # that start with `f` and those that start with `r`

    args_by_prefix: Dict[str, Dict[str, CastableType]] = dict(f={}, r={})
    for k, v in kwargs.items():
        c = k[0]
        if c not in "fr":
            msg = f"bad path containing {c}: must only contain `f` and `r`"
            raise ValueError(msg)
        args_by_prefix[c][k[1:]] = v

    pair = program.pair
    if pair is None:
        raise ValueError("path into atom")

    # recurse down the tree
    new_f = replace(pair[0], **args_by_prefix.get("f", {}))
    new_r = replace(pair[1], **args_by_prefix.get("r", {}))

    return (new_f, new_r)
