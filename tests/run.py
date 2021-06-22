#!/usr/bin/env python3

from clvm_rs import deserialize_and_run_program, STRICT_MODE
from clvm.operators import OP_REWRITE
from clvm import KEYWORD_FROM_ATOM, KEYWORD_TO_ATOM
import sys

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
    max_cost = 11000000000

    return deserialize_and_run_program(
        program_data,
        env_data,
        KEYWORD_TO_ATOM["q"][0],
        KEYWORD_TO_ATOM["a"][0],
        native_opcode_names_by_opcode,
        max_cost,
        0,
    )

if __name__ == "__main__":
    try:
        run_clvm(sys.argv[1], sys.argv[2])
    except Exception as e:
        print("FAIL:", e)
