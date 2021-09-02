#!/usr/bin/env python3

from run_gen import run_gen
from clvm_rs import STRICT_MODE
from time import time
import sys
import glob

failed = 0

def compare_output(output, expected, title):
    if expected != output:
        print(f"{title} output:")
        print(output)
        print("expected:")
        print(expected)
        failed = 1

def parse_output(result, error_code):
    output = ""
    for r in sorted(result, key=lambda x: x.coin_name):
        output += f"coin: {r.coin_name.hex()} ph: {r.puzzle_hash.hex()}\n"
        for c in sorted(r.conditions, key=lambda x: x[0]):
            output += f"  {c[0]}\n"
            for cwa in sorted(c[1], key=lambda x: (x.opcode, x.vars)):
                output += f"    {cwa.opcode}"

                # special case to omit an empty hint from CREATE_COIN, to preserve the
                # output from the test cases not using a hint
                var = list(cwa.vars)
                if cwa.opcode == 51 and len(cwa.vars) == 3 and len(cwa.vars[2]) == 0:
                    var.pop(2)

                for a in var:
                    output += f" {a.hex()}"
                output += "\n"
    if error_code:
        output += f"FAILED: {error_code}\n"
    return output

for g in glob.glob('generators/*.clvm'):
    start_time = time()
    error_code, result, cost = run_gen(g)
    run_time = time() - start_time
    output = parse_output(result, error_code)

    start_time = time()
    error_code2, result2, cost2 = run_gen(g, STRICT_MODE)
    run_time2 = time() - start_time
    output2 = parse_output(result2, error_code2)

    with open(g) as f:
        expected = f.read().split('\n', 1)[1]
        print(f"{g}")
        if not "STRICT" in expected:
            expected2 = expected
            if cost != cost2:
                print("cost when running in strict mode differs from non-strict!")
                failed = 1
        else:
            expected, expected2 = expected.split("STRICT:\n", 1)
        compare_output(output, expected, "")
        print(f"  cost: {cost}")
        print(f"  run-time: {run_time:.2f}s")

        compare_output(output2, expected2, "STRICT")
        print(f"  cost: {cost2}")
        print(f"  run-time: {run_time2:.2f}s")

        limit = 1.5

        # temporary higher limits until this is optimized
        if "duplicate-coin-announce.clvm" in g:
            limit = 9
        elif "negative-reserve-fee.clvm" in g:
            limit = 4
        elif "block-834752" in g:
            limit = 2
        elif "block-834760" in g:
            limit = 10
        elif "block-834765" in g:
            limit = 5
        elif "block-834766" in g:
            limit = 6
        elif "block-834768" in g:
            limit = 6

        if run_time > limit or run_time2 > limit:
            print("run-time exceeds limit!")
            failed = 1

sys.exit(failed)
