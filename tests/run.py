#!/usr/bin/env python3

from clvm_rs import Program


def run_clvm(fn, env=None):

    program = Program.fromhex(open(fn, 'r').read())
    if env is not None:
        env = Program.fromhex(open(env, 'r').read())
    else:
        env = Program.fromhex("ff80")
    # constants from the main chia blockchain:
    # https://github.com/Chia-Network/chia-blockchain/blob/main/chia/consensus/default_constants.py
    max_cost = 11000000000
    cost_per_byte = 12000

    max_cost -= (len(bytes(program)) + len(bytes(env))) * cost_per_byte
    return program.run_with_cost(
        env,
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
        print("FAIL:", e.args[0])
        sys.exit(1)
    start = time()
    ret_size = count_tree_size(result)
    duration = time() - start;
    print(f"returned bytes: {ret_size}")
    print(f"parse return value time: {duration:.2f}s")
    sys.exit(0)
