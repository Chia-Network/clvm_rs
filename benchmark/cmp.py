#!/usr/bin/env python

# tool to compare the output of two benchmark runs

# example input:
#        1509a62d.hex mean: 0.036724 (+/- 0.004144)    0.034569 0.037651 0.034424 0.040868 0.036109
#        54fd91bb.hex mean: 0.021840 (+/- 0.003745)    0.019578 0.020003 0.025585 0.022604 0.021428
#        7312527a.hex mean: 0.026739 (+/- 0.002391)    0.028513 0.027997 0.026683 0.024348 0.026152
#        73d19fac.hex mean: 0.072710 (+/- 0.006377)    0.073508 0.079087 0.070672 0.071331 0.068953
#        90c890db.hex mean: 0.035235 (+/- 0.002725)    0.037361 0.035506 0.033522 0.032510 0.037278
#        a24336b6.hex mean: 0.020211 (+/- 0.002478)    0.019213 0.020572 0.022689 0.017757 0.020822
#        c38a49ea.hex mean: 0.018909 (+/- 0.002474)    0.019311 0.017681 0.021383 0.017074 0.019097
#        ca4b27bc.hex mean: 0.042279 (+/- 0.002610)    0.040601 0.040331 0.040838 0.044889 0.044734
#          concat.hex mean: 0.531042 (+/- 0.144099)    0.675141 0.425383 0.575245 0.476891 0.502552
#        d20e7d20.hex mean: 0.036166 (+/- 0.007266)    0.032407 0.036944 0.035697 0.032349 0.043432
#       factorial.hex mean: 0.156692 (+/- 0.021156)    0.177848 0.154092 0.147916 0.156699 0.146906
#       hash-tree.hex mean: 0.092139 (+/- 0.013170)    0.092613 0.090821 0.087656 0.084295 0.105309
#     large-block.hex mean: 0.338159 (+/- 0.019837)    0.335825 0.357996 0.319529 0.334709 0.342734
# matrix-multiply.hex mean: 0.449414 (+/- 0.044799)    0.425871 0.494213 0.446407 0.432410 0.448168
#       point-pow.hex mean: 1.237281 (+/- 0.023233)    1.227493 1.240997 1.234950 1.222452 1.260514
#      shift-left.hex mean: 2.327137 (+/- 0.145706)    2.472843 2.373162 2.345213 2.235939 2.208530
#     substr-tree.hex mean: 0.476707 (+/- 0.005958)    0.482061 0.479639 0.471037 0.480049 0.470749
#          substr.hex mean: 0.121002 (+/- 0.038429)    0.141937 0.159431 0.092551 0.101701 0.109389
#        sum-tree.hex mean: 0.706867 (+/- 0.044276)    0.749527 0.662591 0.699655 0.718693 0.703870
#      TOTAL: 33.736263 s
#UNCERTAINTY: 0.534874 s

import os
import sys
from colorama import init, Fore, Style

init()

if len(sys.argv) < 3:
    print("usage: cmp.py <before-benchmark> <after-benchmark>")
    sys.exit(1)

run = [{}, {}]
for i in range(2):
    raw = open(sys.argv[i + 1], 'r').read().split('\n')
    raw = raw[raw.index("benchmarking...")+1:]
    tests = {}
    for l in raw:
        l = l.strip().split()
        if len(l) == 0:
            continue
        if l[0] == "TOTAL:":
            run[i]["total"] = float(l[1])
            continue
        if l[0] == "UNCERTAINTY:":
            run[i]["diff"] = float(l[1])
            continue
        name = l[0]
        if l[1] != "mean:" or l[3] != "(+/-":
            print("unexpected input")
            sys.exit(1)
        time = float(l[2])
        diff = float(l[4][:-1])
        tests[name] = { "time": time, "diff": diff }
        run[i]["tests"] = tests

for name, t0 in run[0]["tests"].items():
    t1 = run[1]["tests"][name]
    t1time = t1["time"]
    t1diff = t1["diff"]
    t0time = t0["time"]
    t0diff = t0["diff"]
    if abs(t1time - t0time) < t0diff or abs(t1time - t0time) < t1diff:
        # inconclusive
        print("%20s mean: %f (%+f) %+.2f %% (within uncertainty)" % (name, t1time, t1time - t0time, (t1time - t0time) / t0time * 100.0))
    else:
        if t0time > t1time:
            print(Fore.GREEN, end='')
        else:
            print(Fore.RED, end='')
        print("%20s mean: %f (%+f) %+.2f %%" % (name, t1time, t1time - t0time, (t1time - t0time) / t0time * 100.0))
    print(Fore.RESET, end='')

tot0 = run[0]["total"]
tot1 = run[1]["total"]
tot0diff = run[0]["diff"]
tot1diff = run[1]["diff"]

if abs(tot0 - tot1) < tot0diff or abs(tot0 - tot1) < tot1diff:
    print("TOTAL: %f (%+f) %+.2f %% (within uncertainty)" % (tot1, tot1 - tot0, (tot1 - tot0) / tot0 * 100.0))
else:
    if tot0 > tot1:
        print(Fore.GREEN, end='')
    else:
        print(Fore.RED, end='')
    print("TOTAL: %f (%+f) %+.2f %%" % (tot1, tot1 - tot0, (tot1 - tot0) / tot0 * 100.0))
    print(Fore.RESET, end='')
