from __future__ import annotations
from typing import Iterator, List, Tuple, Optional, BinaryIO

from .at import at
from .casts import CastableType, to_clvm_object, int_from_bytes, int_to_bytes
from .chia_dialect import CHIA_DIALECT
from .clvm_rs import run_serialized_chia_program
from .clvm_storage import CLVMStorage
from .clvm_tree import CLVMTree
from .curry_and_treehash import CurryTreehasher
from .eval_error import EvalError
from .replace import replace
from .ser import sexp_from_stream, sexp_to_stream, sexp_to_bytes
from .tree_hash import sha256_treehash



class Program(CLVMStorage):
    """
    A wrapper around `CLVMStorage` providing many convenience functions.
    """

    UNSAFE_MAX_COST: Optional[int] = None

    curry_treehasher: CurryTreehasher = CurryTreehasher(CHIA_DIALECT)
    _cached_serialization: Optional[bytes]

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
        tree = CLVMTree.from_bytes(
            blob[cursor:], calculate_tree_hash=calculate_tree_hash
        )
        obj = cls.wrap(tree)
        new_cursor = len(bytes(tree)) + cursor
        return obj, new_cursor

    @classmethod
    def fromhex(cls, hexstr: str) -> Program:
        return cls.from_bytes(bytes.fromhex(hexstr))

    def __bytes__(self) -> bytes:
        if self._cached_serialization is None:
            self._cached_serialization = sexp_to_bytes(self)
        if not isinstance(self._cached_serialization, bytes):
            self._cached_serialization = bytes(self._cached_serialization)
        return self._cached_serialization

    def __int__(self) -> int:
        v = self.as_int()
        if v is None:
            raise ValueError("can't cast pair to int")
        return v

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
        self._unwrapped = self
        self._unwrapped_pair = None
        self._cached_serialization = None
        self._cached_sha256_treehash = None

    @property
    def pair(self) -> Optional[Tuple["Program", "Program"]]:
        if self._pair is None and self.atom is None:
            pair = self._unwrapped_pair
            self._pair = (self.wrap(pair[0]), self.wrap(pair[1]))
        return self._pair

    @classmethod
    def to(cls, v: CastableType) -> Program:
        return cls.wrap(to_clvm_object(v, cls.new_atom, cls.new_pair))

    @classmethod
    def wrap(cls, v: CLVMStorage) -> Program:
        if isinstance(v, Program):
            return v
        o = cls()
        o.atom = v.atom
        o._pair = None
        o._unwrapped = v
        o._unwrapped_pair = v.pair
        o._cached_serialization = getattr(v, "_cached_serialization", None)
        o._cached_sha256_treehash = getattr(v, "_cached_sha256_treehash", None)
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

    @classmethod
    def one(cls) -> Program:
        return ONE_PROGRAM

    # display

    def __str__(self) -> str:
        s = bytes(self).hex()
        if len(s) > 76:
            s = f"{s[:70]}...{s[-6:]}"
        return s

    def __repr__(self) -> str:
        return f"{self.__class__.__name__}({str(self)})"

    def __eq__(self, other) -> bool:
        try:
            other_obj = self.to(other)
        except ValueError:
            # cast failure
            return False
        return self.tree_hash() == other_obj.tree_hash()

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
        r = at(self, position)
        if r is None:
            return r
        return self.to(r)

    def at_many(self, *positions: str) -> List[Optional["Program"]]:
        """
        Call `.at` multiple times.

        Why? So you can write

        `
        if p.at_many("f", "rf", "rfrf") == [5, 10, 15]:
        `
        instead of
        `
        if [p.at("f"), p.at("rf"), p.at("rfrf")] == [5, 10, 15]:
        `
        """
        return [self.at(_) for _ in positions]

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

    def tree_hash(self) -> bytes:
        # we operate on the unwrapped version to prevent the re-wrapping that
        # happens on each invocation of `Program.pair` whenever possible
        if self._cached_sha256_treehash is None:
            self._cached_sha256_treehash = sha256_treehash(self._unwrapped)
        return self._cached_sha256_treehash

    def run_with_cost(
        self, args, max_cost: int, flags: int = 0
    ) -> Tuple[int, "Program"]:
        prog_bytes = bytes(self)
        args_bytes = bytes(self.to(args))
        try:
            cost, lazy_node = run_serialized_chia_program(
                prog_bytes, args_bytes, max_cost, flags
            )
            r = self.wrap(lazy_node)
        except ValueError as ve:
            raise EvalError(ve.args[0], self.wrap(ve.args[1]))
        return cost, r

    def run(self, args) -> "Program":
        """
        Run with the default `UNSAFE_MAX_COST` value. Using too high a value with
        misbehaving code may exhaust memory or take a long time.
        """
        max_cost = self.__class__.UNSAFE_MAX_COST
        if max_cost is None:
            raise ValueError("please call `set_run_unsafe_max_cost` before using `run`")
        cost, r = self.run_with_cost(args, max_cost=max_cost)
        return r

    @classmethod
    def set_run_unsafe_max_cost(cls, new_max_cost: int):
        cls.UNSAFE_MAX_COST = new_max_cost

    def curry(self, *args: CastableType) -> "Program":
        """
        Given a `MOD` program, cast to `Program` the list of values and
        bind them to the `MOD`. See also https://docs.chia.net/guides/chialisp-currying

        Returns a program with the given values bound.
        """
        return self.to(self.curry_treehasher.curry(self, *args))

    def uncurry(self) -> Tuple[Program, Optional[List[Program]]]:
        """
        uncurry the given program

        returns `mod, [arg1, arg2, ...]`

        if the program is not a valid curry, return `self, NULL`

        This distinguishes it from the case of a valid curry of 0 arguments
        (which is rather pointless but possible), which returns `self, []`
        """
        mod, args = self.curry_treehasher.uncurry(self)
        p_args = args if args is None else [self.to(_) for _ in args]
        return self.to(mod), p_args

    def curry_hash(self, *args: bytes) -> bytes:
        """
        Return a puzzle hash that would be created if you curried this puzzle
        with arguments that have the given hashes.

        In other words,

        ```
        c1 = self.curry(arg1, arg2, arg3).tree_hash()
        c2 = self.curry_hash(arg1.tree_hash(), arg2.tree_hash(), arg3.tree_hash())
        assert c1 == c2  # they will be the same
        ```

        This looks useless to the unitiated, but sometimes you'll need a puzzle
        hash where you don't actually know the contents of a clvm subtree -- just its
        hash. This lets you calculate the puzzle hash with hidden information.
        """
        curry_treehasher = self.curry_treehasher
        quoted_mod_hash = curry_treehasher.calculate_hash_of_quoted_mod_hash(
            self.tree_hash()
        )
        return curry_treehasher.curry_and_treehash(quoted_mod_hash, *args)

    def as_int(self) -> Optional[int]:
        v = self.atom
        if v is None:
            return v
        return int_from_bytes(v)

    def as_iter(self) -> Iterator[Program]:
        v = self
        while v.pair:
            yield v.pair[0]
            v = v.pair[1]


NULL_PROGRAM = Program.fromhex("80")
ONE_PROGRAM = Program.fromhex("01")
