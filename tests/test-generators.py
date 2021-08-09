#!/usr/bin/env python3

from run_gen import run_gen, print_spend_bundle_conditions
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


def parse_output(result, error_code) -> str:
    if error_code:
        return f"FAILED: {error_code}\n"
    else:
        return print_spend_bundle_conditions(result)

for g in glob.glob('generators/*.clvm'):
    print(f"{g}")
    start_time = time()
    error_code, result = run_gen(g)
    run_time = time() - start_time
    output = parse_output(result, error_code)

    start_time = time()
    error_code2, result2 = run_gen(g, STRICT_MODE)
    run_time2 = time() - start_time
    output2 = parse_output(result2, error_code2)

    with open(g) as f:
        expected = f.read().split('\n', 1)[1]
        if not "STRICT" in expected:
            expected2 = expected
            if not (result is None and result2 is None or result.cost == result2.cost):
                print("cost when running in strict mode differs from non-strict!")
                failed = 1
        else:
            expected, expected2 = expected.split("STRICT:\n", 1)

        compare_output(output, expected, "")
        print(f"  run-time: {run_time:.2f}s")

        compare_output(output2, expected2, "STRICT")
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
