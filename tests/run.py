#!/usr/bin/env python3

from clvm_rs import deserialize_and_run_program2, STRICT_MODE
from clvm.operators import OP_REWRITE
from clvm import KEYWORD_FROM_ATOM, KEYWORD_TO_ATOM

native_opcode_names_by_opcode = dict(
    ("op_%s" % OP_REWRITE.get(k, k), op)
    for op, k in KEYWORD_FROM_ATOM.items()
    if k not in "qa."
)

def run_clvm(fn, env=None):

    program_data = bytes.fromhex(open(fn, 'r').read())
    if env is not None:
        env_data = bytes.fromhex(open(env, 'r').read())
    else:
        env_data = bytes.fromhex("ff80")
    # constants from the main chia blockchain:
    # https://github.com/Chia-Network/chia-blockchain/blob/main/chia/consensus/default_constants.py
    max_cost = 11000000000
    cost_per_byte = 12000

    max_cost -= (len(program_data) + len(env_data)) * cost_per_byte
    return deserialize_and_run_program2(
        program_data,
        env_data,
        KEYWORD_TO_ATOM["q"][0],
        KEYWORD_TO_ATOM["a"][0],
        native_opcode_names_by_opcode,
        max_cost,
        0,
    )

def count_tree_size(tree) -> int:
    stack = [tree]
    ret = 0
    while len(stack):
        i = stack.pop()
        if i.atom is not None:
            ret += len(i.atom)
        elif i.pair is not None:
            stack.append(i.pair[1])
            stack.append(i.pair[0])
        else:
            # this shouldn't happen
            assert False
    return ret

if __name__ == "__main__":
    import sys
    from time import time

    try:
        start = time()
        cost, result = run_clvm(sys.argv[1], sys.argv[2])
        duration = time() - start;
        print(f"cost: {cost}")
        print(f"execution time: {duration:.2f}s")
    except Exception as e:
        print("FAIL:", e)
        sys.exit(1)
    start = time()
    ret_size = count_tree_size(result)
    duration = time() - start;
    print(f"returned bytes: {ret_size}")
    print(f"parse return value time: {duration:.2f}s")
    sys.exit(0)
