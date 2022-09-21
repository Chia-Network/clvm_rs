from typing import Any, Optional, Tuple

CLVM = Any


def at(sexp: CLVM, position: str) -> Optional[CLVM]:
    """
    Take a string of only `f` and `r` characters and follow the corresponding path.

    Example:

    `assert Program.to(17) == Program.to([10, 20, 30, [15, 17], 40, 50]).at("rrrfrf")`

    """
    v = sexp
    for c in position.lower():
        p = v.pair
        if p is None:
            return p
        if c not in "rf":
            raise ValueError(
                f"`at` got illegal character `{c}`. Only `f` & `r` allowed"
            )
        v = p[0 if c == "f" else 1]
    return v


# Replicates the curry function from clvm_tools, taking advantage of *args
# being a list.  We iterate through args in reverse building the code to
# create a clvm list.
#
# Given arguments to a function addressable by the '1' reference in clvm
#
# fixed_args = 1
#
# Each arg is prepended as fixed_args = (c (q . arg) fixed_args)
#
# The resulting argument list is interpreted with apply (2)
#
# (2 (1 . self) rest)
#
# Resulting in a function which places its own arguments after those
# curried in in the form of a proper list.


def curry(sexp: CLVM, *args) -> CLVM:
    fixed_args: Any = 1
    while args:
        arg = args.pop()
        fixed_args = [4, (1, arg), fixed_args]
    return sexp.to([2, (1, sexp), fixed_args])


# UNCURRY_PATTERN_FUNCTION = assemble("(a (q . (: . function)) (: . core))")
# UNCURRY_PATTERN_CORE = assemble("(c (q . (: . parm)) (: . core))")


ONE_PATH = Q_KW = bytes([1])
C_KW = bytes([2])
A_KW = bytes([4])
NULL = bytes([])


def uncurry(sexp: CLVM) -> Optional[Tuple[CLVM, CLVM]]:
    if (
        at(sexp, "f").atom != A_KW
        or at(sexp, "rf").atom != Q_KW
        or at(sexp, "rrr").atom != NULL
    ):
        return None
    uncurried_function = at(sexp, "rr")
    core_items = []
    core = at(sexp, "rrf")
    while core.atom != ONE_PATH:
        if (
            at(core, "f").atom != C_KW
            or at(core, "rf").atom != Q_KW
            or at(sexp, "rrr").atom != NULL
        ):
            return None
        new_item = at(core, "rr")
        core_items.append(new_item)
        core = at(core, "rrf")
    core_items.reverse()
    return uncurried_function, core_items
