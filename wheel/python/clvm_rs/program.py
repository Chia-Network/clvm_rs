from __future__ import annotations
from typing import Iterator, List, Tuple, Optional, Any, BinaryIO

from .at import at
from .bytes32 import bytes32
from .casts import to_clvm_object, int_from_bytes, int_to_bytes
from .clvm_rs import run_serialized_chia_program
from .clvm_storage import CLVMStorage
from .clvm_tree import CLVMTree
from .curry_and_treehash import CurryTreehasher, CHIA_CURRY_TREEHASHER
from .eval_error import EvalError
from .replace import replace
from .ser import sexp_from_stream, sexp_to_stream, sexp_to_bytes
from .tree_hash import sha256_treehash


MAX_COST = 0x7FFFFFFFFFFFFFFF


class Program(CLVMStorage):
    """
    A thin wrapper around s-expression data intended to be invoked with "eval".
    """

    curry_treehasher: CurryTreehasher = CHIA_CURRY_TREEHASHER

    # serialization/deserialization

    @classmethod
    def parse(cls, f: BinaryIO) -> Program:
        return sexp_from_stream(f, cls.new_pair, cls.new_atom)

    def stream(self, f: BinaryIO) -> None:
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
        return id(self)

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
        return self.to(at(self, position))

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
        return self.to(replace(self, **kwargs))

    def tree_hash(self) -> bytes32:
        return sha256_treehash(self)

    def run_with_cost(
        self, args, max_cost: int = MAX_COST, flags: int = 0
    ) -> Tuple[int, "Program"]:
        prog_bytes = bytes(self)
        args_bytes = bytes(self.to(args))
        try:
            cost, r = run_serialized_chia_program(
                prog_bytes, args_bytes, max_cost, flags
            )
            r = self.wrap(r)
        except ValueError as ve:
            raise EvalError(ve.args[0], self.wrap(ve.args[1]))
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
        return self.to(self.curry_treehasher.curry(self, *args))

    """
    uncurry the given program

    returns `mod, [arg1, arg2, ...]`

    if the program is not a valid curry, return `self, NULL`

    This distinguishes it from the case of a valid curry of 0 arguments
    (which is rather pointless but possible), which returns `self, []`
    """

    def uncurry(self) -> Tuple[Program, Optional[List[Program]]]:
        mod, args = self.curry_treehasher.uncurry(self)
        p_args = args if args is None else [self.to(_) for _ in args]
        return self.to(mod), p_args

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
