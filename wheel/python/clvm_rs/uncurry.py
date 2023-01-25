from __future__ import annotations
from typing import List, Tuple, Optional

from .at import at
from .chia_dialect import Dialect
from .clvm_storage import CLVMStorage


"""
uncurry the given program

returns `mod, [arg1, arg2, ...]`

if the program is not a valid curry, return `sexp, NULL`

This distinguishes it from the case of a valid curry of 0 arguments
(which is rather pointless but possible), which returns `sexp, []`
"""


def uncurry(
    dialect: Dialect, sexp: CLVMStorage
) -> Tuple[CLVMStorage, Optional[List[CLVMStorage]]]:
    if (
        at(sexp, "f") != dialect.A_KW
        or at(sexp, "rff") != dialect.Q_KW
        or at(sexp, "rrr") != dialect.NULL
    ):
        return sexp, None
    # since "rff" is not None, neither is "rfr"
    uncurried_function = at(sexp, "rfr")
    assert uncurried_function is not None
    core_items = []

    # since "rrr" is not None, neither is rrf
    core = at(sexp, "rrf")
    while core != dialect.ONE:
        assert core is not None
        if (
            at(core, "f") != dialect.C_KW
            or at(core, "rff") != dialect.Q_KW
            or at(core, "rrr") != dialect.NULL
        ):
            return sexp, None
        # since "rff" is not None, neither is "rfr"
        new_item = at(core, "rfr")
        assert new_item is not None
        core_items.append(new_item)
        # since "rrr" is not None, neither is rrf
        core = at(core, "rrf")
    return uncurried_function, core_items
