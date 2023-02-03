import io
import pathlib
import time

from clvm_rs.program import Program
from clvm_rs.clvm_rs import serialized_length


def bench(f, name: str, allow_slow=False):
    r, t = bench_w_speed(f, name)
    if not allow_slow and t > 0.01:
        print("*** TOO SLOW")
    print()
    return r


def bench_w_speed(f, name: str):
    start = time.time()
    r = f()
    end = time.time()
    d = end - start
    print(f"{name}: {d:1.4f} s")
    return r, d


def benchmark():
    block_path = pathlib.Path(__file__).parent / "block-2500014.compressed.bin"
    obj = bench(
        lambda: Program.parse(open(block_path, "rb")),
        "obj = Program.parse(open([block_blob]))",
    )
    bench(lambda: bytes(obj), "bytes(obj)")

    block_blob = open(block_path, "rb").read()
    obj1 = bench(
        lambda: Program.from_bytes(block_blob),
        "obj = Program.from_bytes([block_blob])",
    )
    bench(lambda: bytes(obj1), "bytes(obj)")

    cost, output = bench(lambda: obj.run_with_cost(0), "run", allow_slow=True)
    print(f"cost = {cost}")
    result_blob = bench(
        lambda: bytes(output),
        "serialize LazyNode",
        allow_slow=True,
    )
    print(f"output = {len(result_blob)}"),

    result_blob_2 = bench(lambda: bytes(output), "serialize LazyNode again")
    assert result_blob == result_blob_2

    bench(
        lambda: print(output.tree_hash().hex()),
        "tree hash LazyNode",
        allow_slow=True,
    )
    bench(lambda: print(output.tree_hash().hex()), "tree hash again LazyNode")

    des_output = bench(
        lambda: Program.from_bytes(result_blob),
        "from_bytes (with tree hashing)",
        allow_slow=True,
    )
    bench(lambda: des_output.tree_hash(), "from_bytes (with tree hashing) tree hash")
    bench(
        lambda: des_output.tree_hash(), "from_bytes (with tree hashing) tree hash again"
    )

    bench(lambda: serialized_length(result_blob), "serialized_length")

    des_output = bench(
        lambda: Program.from_bytes(result_blob, calculate_tree_hash=False),
        "from_bytes output (without tree hashing)",
        allow_slow=True,
    )
    bench(
        lambda: des_output.tree_hash(),
        "from_bytes (without tree hashing) tree hash",
        allow_slow=True,
    )
    bench(
        lambda: des_output.tree_hash(),
        "from_bytes (without tree hashing) tree hash again",
    )

    reparsed_output = bench(
        lambda: Program.parse(io.BytesIO(result_blob)),
        "reparse output",
        allow_slow=True,
    )
    bench(lambda: reparsed_output.tree_hash(), "reparsed tree hash", allow_slow=True)
    bench(
        lambda: reparsed_output.tree_hash(),
        "reparsed tree hash again",
    )

    foo = Program.to("foo")
    o0 = Program.to((foo, obj))
    o1 = Program.to((foo, obj1))

    def compare():
        assert o0 == o1

    bench(compare, "compare")

    bench(lambda: bytes(o0), "to_bytes o0")
    bench(lambda: bytes(o1), "to_bytes o1")

    bench(lambda: print(o0.tree_hash().hex()), "o0 tree_hash")
    bench(lambda: print(o0.tree_hash().hex()), "o0 tree_hash (again)")

    bench(lambda: print(o1.tree_hash().hex()), "o1 tree_hash")
    bench(lambda: print(o1.tree_hash().hex()), "o1 tree_hash (again)")

    o2 = Program.to((foo, output))

    bench(lambda: print(o2.tree_hash().hex()), "o2 tree_hash")
    bench(lambda: print(o2.tree_hash().hex()), "o2 tree_hash (again)")

    # start = time.time()
    # obj1 = sexp_from_stream(io.BytesIO(out), SExp.to, allow_backrefs=False)
    # end = time.time()
    # print(end-start)


if __name__ == "__main__":
    benchmark()
