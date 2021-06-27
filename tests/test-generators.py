#!/usr/bin/env python3

from run_gen import run_gen
from time import time
import sys
import glob

failed = 0

for g in glob.glob('generators/*.clvm'):
    start_time = time()
    error_code, result, cost = run_gen(g)
    run_time = time() - start_time

    output = ""
    for r in sorted(result):
        output += f"coin: {r.coin_name.hex()} ph: {r.puzzle_hash.hex()}\n"
        for c in sorted(r.conditions):
            output += f"  {c[0]}\n"
            for cwa in sorted(c[1], key=lambda x: (x.opcode, x.vars)):
                output += f"    {cwa.opcode}"
                for a in cwa.vars:
                    output += f" {a.hex()}"
                output += "\n"
    if error_code:
        output += f"FAILED: {error_code}\n"
    with open(g) as f:
        expected = f.read().split('\n', 1)[1]
        print(f"{g}")
        if expected != output:
            print(f"output:")
            print(output)
            print("expected:")
            print(expected)
            failed = 1
        print(f"  cost: {cost}")
        print(f"  run-time: {run_time:.2f}s")
        if run_time > 1:
            print("run-time exceeds limit!")
            failed = 1

sys.exit(failed)
