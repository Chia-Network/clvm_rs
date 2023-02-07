import io
import time

from clvm_rs.program import Program



def bench(f, name: str):
    start = time.time()
    r = f()
    end = time.time()
    d = end - start
    print(f"{name}: {end-start:1.4f} s")
    print()
    return r



sha_prog = Program.fromhex("ff0bff0180")

print(sha_prog.run("food"))
#breakpoint()


obj = bench(lambda: Program.parse(open("block-2500014.compressed.bin", "rb")), "parse")
bench(lambda: bytes(obj), "to_bytes")

obj1 = bench(lambda: Program.from_bytes(open("block-2500014.compressed.bin", "rb").read()), "from_bytes")
bench(lambda: bytes(obj1), "to_bytes")

cost, output = bench(lambda: obj.run_with_cost(0), "run")

print(f"cost = {cost}")
blob = bench(lambda: print(f"output = {len(bytes(output))}"), "serialize LazyNode")
blob = bench(lambda: bytes(output), "serialize LazyNode again")

bench(lambda: print(output.tree_hash().hex()), "print run tree hash LazyNode")
bench(lambda: print(output.tree_hash().hex()), "print run tree hash again LazyNode")

des_output = bench(lambda: Program.from_bytes(blob), "from_bytes output")
bench(lambda: print(des_output.tree_hash().hex()), "print from_bytes tree hash")
bench(lambda: print(des_output.tree_hash().hex()), "print from_bytes tree hash again")

reparsed_output = bench(lambda: Program.parse(io.BytesIO(blob)), "parse output")
bench(lambda: print(reparsed_output.tree_hash().hex()), "print parsed tree hash")
bench(lambda: print(reparsed_output.tree_hash().hex()), "print parsed tree hash again")


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
# obj1 = sexp_from_stream(io.BytesIO(out), SExp.to, allow_backrefs=True)
# end = time.time()
# print(end-start)
