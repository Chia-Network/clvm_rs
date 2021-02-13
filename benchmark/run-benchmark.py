#!/usr/bin/env python

import glob
import subprocess
import sys
import os
import time
import random
from clvm_rs import serialize_and_run_program, STRICT_MODE
from colorama import init, Fore, Style

init()

procs = []

def long_string(filename):
    if "-v" in sys.argv:
        print("generating %s" % filename)
    with open(filename, 'w+') as f:
        f.write('("')
        for i in range(1000):
            f.write('abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789')
        f.write('")')

def _large_tree_impl(f, depth):
    if depth == 0:
        f.write('%d' % random.getrandbits(64))
    else:
        f.write('(')
        _large_tree_impl(f, depth - 1)
        f.write(' . ')
        _large_tree_impl(f, depth - 1)
        f.write(')')

def large_tree(filename):
    if "-v" in sys.argv:
        print("generating %s" % filename)
    with open(filename, 'w+') as f:
        _large_tree_impl(f, 19)

print('generating...')
if not os.path.exists('benchmark/substr.env'):
    long_string('benchmark/substr.env')

if not os.path.exists('benchmark/substr-tree.env'):
    long_string('benchmark/substr-tree.env')

if not os.path.exists('benchmark/sum-tree.env'):
    large_tree('benchmark/sum-tree.env')

print('compiling...')
for fn in glob.glob('benchmark/*.clvm'):

    hex_name = fn[:-4] + 'hex'
    if not os.path.exists(hex_name):
        out = open(hex_name, 'w+')
        if "-v" in sys.argv:
            print("opc %s" % fn)
        proc = subprocess.Popen(['opc', fn], stdout=out)
        procs.append(proc)

    env_hex_name = fn[:-4] + 'envhex'
    if not os.path.exists(env_hex_name):
        out = open(env_hex_name, 'w+')
        if "-v" in sys.argv:
            print("opc %s" % (fn[:-4] + 'env'))
        proc = subprocess.Popen(['opc', fn[:-4] + 'env'], stdout=out)
        procs.append(proc)

if len(procs) > 0:
    print("[" + (" " * len(procs)) + "]\r[", end="")
    for p in procs:
        p.wait()
        print(".", end="")
        sys.stdout.flush()
    print("")

test_runs = {}

print('benchmarking...')
for n in range(5):
    if "-v" in sys.argv:
        print('pass %d' % n)
    for fn in glob.glob('benchmark/*.hex'):
        env_fn = fn[:-3] + 'envhex'

        max_cost = 0
        flags = 0
        if '--brun' in sys.argv:
            command = ['brun', '-c', '--backend=rust', '--quiet', '--time', '--hex', fn, env_fn]
            if "-v" in sys.argv:
                print(" ".join(command))
            output = subprocess.check_output(command)
            output = output.decode('ascii').split('\n', 5)[:-1]
        else:
            program_data = bytes.fromhex(open(fn, 'r').read())
            env_data = bytes.fromhex(open(env_fn, 'r').read())
            time_start = time.perf_counter()
            cost, result = serialize_and_run_program(
                program_data, env_data, 1, 3, max_cost, flags)
            time_end = time.perf_counter()
            output = ["run_program: %f" % (time_end - time_start)]

        counters = {}
        for o in output:
            try:
                if ':' in o:
                    key, value = o.split(':')
                    counters[key.strip()] = value.strip()
                elif '=' in o:
                    key, value = o.split('=')
                    counters[key.strip()] = value.strip()
            except BaseException as e:
                print(e)
                print('ERROR parsing: %s' % o)

        _, fn = os.path.split(fn)
        if fn in test_runs:
            test_runs[fn].append(counters['run_program'])
        else:
            test_runs[fn] = [counters['run_program']]

sum_time = 0.0
sum_uncertainty = 0.0
for n, vals in sorted(test_runs.items()):
    print('%20s' % n, end='')
    mean = 0.0
    for v in vals:
        mean += float(v)
        sum_time += float(v)
    mean /= len(vals)

    diff = 0.0
    for v in vals:
        diff = max(abs(mean - float(v)), diff)
    print(' mean: %f (+/- %f)   ' % (mean, diff), end='')
    sum_uncertainty += diff

    print(Fore.MAGENTA, end='')
    for v in vals:
        print(' %s' % v, end='')
    print(Fore.RESET)

print(Fore.GREEN + '      TOTAL:' + Style.RESET_ALL + ' %f s' % sum_time)
print(Fore.GREEN + 'UNCERTAINTY:' + Style.RESET_ALL + ' %f s' % sum_uncertainty)
