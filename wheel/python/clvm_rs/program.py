from __future__ import annotations
from typing import Dict, Iterator, List, Tuple, Optional, Any

from .clvm_storage import CLVMStorage
from .casts import to_clvm_object, int_from_bytes, int_to_bytes
from .eval_error import EvalError
from clvm_rs.clvm_rs import run_serialized_program
from clvm_rs.serialize import sexp_from_stream, sexp_to_stream, sexp_to_bytes
from clvm_rs.tree_hash import sha256_treehash
from .clvm_tree import CLVMTree
from .bytes32 import bytes32
from .keywords import NULL, ONE, Q_KW, A_KW, C_KW


MAX_COST = 0x7FFFFFFFFFFFFFFF


class Program(CLVMStorage):
    """
    A thin wrapper around s-expression data intended to be invoked with "eval".
    """

    # serialization/deserialization

    @classmethod
    def parse(cls, f) -> Program:
        return sexp_from_stream(f, cls.new_pair, cls.new_atom)

    def stream(self, f):
        sexp_to_stream(self, f)

    @classmethod
    def from_bytes(cls, blob: bytes, calculate_tree_hash: bool = True) -> Program:
        obj, cursor = cls.from_bytes_with_cursor(
            blob, 0, calculate_tree_hash=calculate_tree_hash
        )
        return obj

    @classmethod
    def from_bytes_with_cursor(
        cls, blob: bytes, cursor: int, calculate_tree_hash: bool = True
    ) -> Tuple[Program, int]:
        tree = CLVMTree.from_bytes(blob, calculate_tree_hash=calculate_tree_hash)
        obj = cls.wrap(tree)
        new_cursor = len(bytes(tree)) + cursor
        return obj, new_cursor

    @classmethod
    def fromhex(cls, hexstr: str) -> Program:
        return cls.from_bytes(bytes.fromhex(hexstr))

    def __bytes__(self) -> bytes:
        return sexp_to_bytes(self)

    def __int__(self) -> int:
        return self.as_int()

    def __hash__(self):
        return self.tree_hash().__hash__()

    @classmethod
    def int_from_bytes(cls, b: bytes) -> int:
        return int_from_bytes(b)

    @classmethod
    def int_to_bytes(cls, i: int) -> bytes:
        return int_to_bytes(i)

    # high level casting with `.to`

    def __init__(self):
        self.atom = b""
        self._pair = None
        self._unwrapped_pair = None
        self._cached_sha256_treehash = None

    @property
    def pair(self) -> Optional[Tuple["Program", "Program"]]:
        if self._pair is None and self.atom is None:
            pair = self._unwrapped_pair
            self._pair = (self.wrap(pair[0]), self.wrap(pair[1]))
        return self._pair

    @classmethod
    def to(cls, v: Any) -> Program:
        return cls.wrap(to_clvm_object(v, cls.new_atom, cls.new_pair))

    @classmethod
    def wrap(cls, v: CLVMStorage) -> Program:
        if isinstance(v, Program):
            return v
        o = cls()
        o.atom = v.atom
        o._pair = None
        o._unwrapped_pair = v.pair
        return o

    # new object creation on the python heap

    @classmethod
    def new_atom(cls, v: bytes) -> Program:
        o = cls()
        o.atom = bytes(v)
        o._pair = None
        o._unwrapped_pair = None
        return o

    @classmethod
    def new_pair(cls, left: CLVMStorage, right: CLVMStorage) -> Program:
        o = cls()
        o.atom = None
        o._pair = None
        o._unwrapped_pair = (left, right)
        return o

    @classmethod
    def null(cls) -> Program:
        return NULL_PROGRAM

    # display

    def __str__(self) -> str:
        return bytes(self).hex()

    def __repr__(self) -> str:
        return f"{self.__class__.__name__}({str(self)})"

    def __eq__(self, other) -> bool:
        stack: List[Tuple[CLVMStorage, CLVMStorage]] = [(self, Program.to(other))]
        while stack:
            p1, p2 = stack.pop()
            if p1.atom is None:
                if p2.atom is not None:
                    return False
                pair_1 = p1.pair
                pair_2 = p2.pair
                assert pair_1 is not None
                assert pair_2 is not None
                stack.append((pair_1[1], pair_2[1]))
                stack.append((pair_1[0], pair_2[0]))
            else:
                if p1.atom != p2.atom:
                    return False
        return True

    def __ne__(self, other) -> bool:
        return not self.__eq__(other)

    def first(self) -> Optional[Program]:
        if self.pair:
            return self.pair[0]
        return None

    def rest(self) -> Optional[Program]:
        if self.pair:
            return self.pair[1]
        return None

    def as_pair(self) -> Optional[Tuple[Program, Program]]:
        return self.pair

    def as_atom(self) -> Optional[bytes]:
        return self.atom

    def listp(self) -> bool:
        return self.pair is not None

    def nullp(self) -> bool:
        return self.atom == b""

    def list_len(self) -> int:
        c = 0
        v: CLVMStorage = self
        while v.pair is not None:
            v = v.pair[1]
            c += 1
        return c

    def at(self, position: str) -> Optional["Program"]:
        """
        Take a string of `f` and `r` characters and follow that path.

        Example:

        ```
        p1 = Program.to([10, 20, 30, [15, 17], 40, 50])
        assert Program.to(17) == at(p1, "rrrfrf")
        ```

        Returns `None` if an atom is hit at some intermediate node.

        ```
        p1 = Program.to(10)
        assert None == at(p1, "rr")
        ```

        """
        v: Optional[Program] = self
        for c in position.lower():
            if v is None:
                return v
            if c == "f":
                v = v.first()
            elif c == "r":
                v = v.rest()
            else:
                raise ValueError(
                    f"`at` got illegal character `{c}`. Only `f` & `r` allowed"
                )
        return v

    def replace(self, **kwargs) -> "Program":
        """
        Create a new program replacing the given paths (using `at` syntax).
        Example:
        ```
        >>> p1 = Program.to([100, 200, 300])
        >>> print(p1.replace(f=105) == Program.to([105, 200, 300]))
        True
        >>> p2 = [100, 200, [301, 302]]
        >>> print(p1.replace(rrf=[301, 302]) == Program.to(p2))
        True
        >>> p2 = [105, 200, [301, 302]]
        >>> print(p1.replace(f=105, rrf=[301, 302]) == Program.to(p2))
        True
        ```

        This is a convenience method intended for use in the wallet or
        command-line hacks where it would be easier to morph elements
        of an existing clvm object tree than to rebuild one from scratch.

        Note that `Program` objects are immutable. This function returns a
        new object; the original is left as-is.
        """
        return _replace(self, **kwargs)

    def tree_hash(self) -> bytes32:
        return sha256_treehash(self)

    def run_with_cost(self, args, max_cost: int = MAX_COST) -> Tuple[int, "Program"]:
        prog_bytes = bytes(self)
        args_bytes = bytes(self.to(args))
        cost, r = run_serialized_program(prog_bytes, args_bytes, max_cost, 0)
        r = self.wrap(r)
        if isinstance(cost, str):
            raise EvalError(cost, r)
        return cost, r

    def run(self, args) -> "Program":
        cost, r = self.run_with_cost(args, MAX_COST)
        return r

    """
    Replicates the curry function from clvm_tools, taking advantage of *args
    being a list.  We iterate through args in reverse building the code to
    create a clvm list.

    Given arguments to a function addressable by the '1' reference in clvm

    fixed_args = 1

    Each arg is prepended as fixed_args = (c (q . arg) fixed_args)

    The resulting argument list is interpreted with apply (2)

    (2 (1 . self) rest)

    Resulting in a function which places its own arguments after those
    curried in in the form of a proper list.
    """

    def curry(self, *args) -> "Program":
        fixed_args: Any = 1
        for arg in reversed(args):
            fixed_args = [4, (1, arg), fixed_args]
        return self.to([2, (1, self), fixed_args])

    """
    uncurry the given program

    returns `mod, [arg1, arg2, ...]`

    if the program is not a valid curry, return `self, NULL`

    This distinguishes it from the case of a valid curry of 0 arguments
    (which is rather pointless but possible), which returns `self, []`
    """

    def uncurry(self) -> Tuple[Program, Optional[List[Program]]]:
        if self.at("f") != A_KW or self.at("rff") != Q_KW or self.at("rrr") != NULL:
            return self, None
        uncurried_function = self.at("rfr")
        if uncurried_function is None:
            return self, None
        core_items = []
        core = self.at("rrf")
        while core != ONE:
            if core is None:
                return self, None
            if core.at("f") != C_KW or core.at("rff") != Q_KW or core.at("rrr") != NULL:
                return self, None
            new_item = core.at("rfr")
            if new_item is None:
                return self, None
            core_items.append(new_item)
            core = core.at("rrf")
        return uncurried_function, core_items

    def as_int(self) -> int:
        return int_from_bytes(self.as_atom())

    def as_iter(self) -> Iterator[Program]:
        v = self
        while v.pair:
            yield v.pair[0]
            v = v.pair[1]

    def as_atom_iter(self) -> Iterator[bytes]:
        """
        Pretend `self` is a list of atoms. Yield the corresponding atoms.

        At each step, we always assume a node to be an atom or a pair.
        If the assumption is wrong, we exit early. This way we never fail
        and always return SOMETHING.
        """
        obj = self
        while obj.pair is not None:
            left, obj = obj.pair
            atom = left.atom
            if atom is None:
                break
            yield atom

    def as_atom_list(self) -> List[bytes]:
        """
        Pretend `self` is a list of atoms. Return the corresponding
        python list of atoms.

        At each step, we always assume a node to be an atom or a pair.
        If the assumption is wrong, we exit early. This way we never fail
        and always return SOMETHING.
        """
        return list(self.as_atom_iter())


NULL_PROGRAM = Program.fromhex("80")


def _replace(program: Program, **kwargs) -> Program:
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

    args_by_prefix: Dict[str, Dict[str, Program]] = dict(f={}, r={})
    for k, v in kwargs.items():
        c = k[0]
        if c not in "fr":
            raise ValueError(f"bad path containing {c}: must only contain `f` and `r`")
        args_by_prefix[c][k[1:]] = program.to(v)

    pair = program.pair
    if pair is None:
        raise ValueError("path into atom")

    # recurse down the tree
    new_f = _replace(pair[0], **args_by_prefix.get("f", {}))
    new_r = _replace(pair[1], **args_by_prefix.get("r", {}))

    return program.new_pair(new_f, new_r)
