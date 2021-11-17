import hashlib
from ir import reader
from clvm_tools import binutils

OPERATORS = [
 ("q", 1),
 ("a", 3),
 ("i", 3),
 ("c", 2),
 ("f", 1),
 ("r", 2),
 ("l", 2),
 ("x", 1),
 ("=", 2),
 ("sha256", 2),
 ("+", 2),
 ("-", 2),
 ("*", 2),
 ("divmod", 2),
 ("div", 2),
 ("substr", 3),
 ("strlen", 1),
 ("point_add", 2),
 ("pubkey_for_exp", 2),
 ("concat", 2),
 (">", 2),
 (">s", 2),
 ("logand", 2),
 ("logior", 2),
 ("logxor", 2),
 ("lognot", 2),
 ("ash", 2),
 ("lsh", 2),
 ("not", 1),
 ("any", 2),
 ("all", 2),
 ("softfork", 3),
]

def arguments():
    for v in [0, 1, -1]:
        yield v
    yield '"FOOBAR"'
    yield "(q 1 2 3)"
    yield "(q . ())"
    for sign in [1, -1]:
        for l in [0, 8, 16, 32, 64]:
            for v in [0x80, 0x100]:
                yield (v << l) * sign
                yield ((v << l) - 1) * sign

def gen_args(max_args):
    # 0 args
    yield ""

    if max_args == 0:
        return

    # 1 argument
    for a in arguments():
        yield f" {a}"

    if max_args == 1:
        return

    # 2 arguments
    for a0 in arguments():
        for a1 in arguments():
            yield f" {a0} {a1}"

    if max_args == 2:
        return

    # 3 arguments
    for a0 in arguments():
        for a1 in arguments():
            for a2 in arguments():
                yield f" {a0} {a1} {a2}"

for op, max_args in OPERATORS:
    for args in gen_args(max_args):
        prg = f"({op}{args})"
        ir_sexp = reader.read_ir(prg)
        prg = binutils.assemble_from_ir(ir_sexp).as_bin()
        h = hashlib.sha1()
        h.update(prg)
        name = h.digest().hex()
        with open(f"corpus/fuzz_run_program/{name}", "wb+") as f:
            f.write(prg)

