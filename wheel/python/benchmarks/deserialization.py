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
        "from_bytes with tree hashing (fbwth)",
        allow_slow=True,
    )
    bench(lambda: des_output.tree_hash(), "tree hash (fbwth)")
    bench(lambda: des_output.tree_hash(), "tree hash again (fbwth)")

    bench(lambda: serialized_length(result_blob), "serialized_length")

    des_output = bench(
        lambda: Program.from_bytes(result_blob, calculate_tree_hash=False),
        "from_bytes without tree hashing (fbwoth)",
        allow_slow=True,
    )
    bench(
        lambda: des_output.tree_hash(),
        "tree hash (fbwoth)",
        allow_slow=True,
    )
    bench(
        lambda: des_output.tree_hash(),
        "tree hash (fbwoth) again",
    )

    reparsed_output = bench(
        lambda: Program.parse(io.BytesIO(result_blob)),
        "parse with tree hashing (pwth)",
        allow_slow=True,
    )
    bench(lambda: reparsed_output.tree_hash(), "tree hash (pwth)")
    bench(
        lambda: reparsed_output.tree_hash(),
        "tree hash again (pwth)",
    )

    reparsed_output = bench(
        lambda: Program.parse(io.BytesIO(result_blob), calculate_tree_hash=False),
        "parse without treehashing (pwowt)",
        allow_slow=True,
    )
    bench(lambda: reparsed_output.tree_hash(), "tree hash (pwowt)", allow_slow=True)
    bench(
        lambda: reparsed_output.tree_hash(),
        "tree hash again (pwowt)",
    )

    foo = Program.to("foo")
    o0 = Program.to((foo, obj))
    o1 = Program.to((foo, obj1))
    o2 = Program.to((foo, output))

    def compare():
        assert o0 == o1

    bench(compare, "compare constructed")

    bench(lambda: bytes(o0), "to_bytes constructed o0")
    bench(lambda: bytes(o1), "to_bytes constructed o1")

    bench(lambda: print(o0.tree_hash().hex()), "o0 tree_hash")
    bench(lambda: print(o0.tree_hash().hex()), "o0 tree_hash (again)")

    bench(lambda: print(o1.tree_hash().hex()), "o1 tree_hash")
    bench(lambda: print(o1.tree_hash().hex()), "o1 tree_hash (again)")

    bench(lambda: bytes(o2), "to_bytes constructed o2")
    bench(lambda: print(o2.tree_hash().hex()), "o2 tree_hash")
    bench(lambda: print(o2.tree_hash().hex()), "o2 tree_hash (again)")


if __name__ == "__main__":
    benchmark()
