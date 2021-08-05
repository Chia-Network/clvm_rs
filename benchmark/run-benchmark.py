#!/usr/bin/env python

import glob
import subprocess
import sys
import os
import time
import random
from clvm import KEYWORD_FROM_ATOM, KEYWORD_TO_ATOM
from clvm.operators import OP_REWRITE
from clvm_rs import deserialize_and_run_program2, STRICT_MODE
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

def long_strings(filename, num):
    if "-v" in sys.argv:
        print("generating %s" % filename)
    with open(filename, 'w+') as f:
        f.write('((')
        for k in range(num):
            f.write('"')
            for i in range(1000):
                f.write('abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789')
            f.write('" ')
        f.write('))')

def _large_tree_impl(f, depth):
    if depth == 0:
        f.write('%d' % random.getrandbits(64))
    else:
        f.write('(')
        _large_tree_impl(f, depth - 1)
        f.write(' . ')
        _large_tree_impl(f, depth - 1)
        f.write(')')

def large_tree(filename, depth=19):
    if "-v" in sys.argv:
        print("generating %s" % filename)
    with open(filename, 'w+') as f:
        _large_tree_impl(f, depth)

def random_key(size=32):
    ret = '0x'
    for i in range(size):
        ret += '%02x' % random.getrandbits(8)
    return ret

def p2_delegated_or_hidden_puzzle():
    # src/wallet/puzzles/p2_delegated_puzzle_or_hidden_puzzle.clvm
    # parameters:
    # (synthetic_public_key original_public_key delegated_puzzle solution)

    return '(a (q 2 (q 2 (i 11 (q 2 (i (= 5 (point_add 11 ' \
        '(pubkey_for_exp (sha256 11 (a 6 (c 2 (c 23 ()))))))) ' \
        '(q 2 23 47) (q 8)) 1) (q 4 (c 4 (c 5 (c (a 6 (c 2 ' \
        '(c 23 ()))) ()))) (a 23 47))) 1) (c (q 50 2 (i (l 5) ' \
        '(q 11 (q . 2) (a 6 (c 2 (c 9 ()))) (a 6 (c 2 (c 13 ())))) ' \
        '(q 11 (q . 1) 5)) 1) 1)) (c (q . %s) 1))' % random_key()

def transaction(puzzle):
    return '(%s (%s (() (q (51 %s %s)) ())))' \
        % (random_key(), puzzle(), random_key(), random_key(3))

def generate_block(filename, puzzle):
    if "-v" in sys.argv:
        print("generating %s" % filename)
    with open(filename, 'w+') as f:
        f.write('(q ')
        for i in range(1000):
            f.write(transaction(puzzle))
        f.write(')')

def generate_list(filename, size):
    if "-v" in sys.argv:
        print("generating %s" % filename)
    with open(filename, 'w+') as f:
        f.write('ff')
        for i in range(size):
            f.write('ff%02x' % random.getrandbits(7))
        for i in range(size):
            f.write('80')
        f.write('80')

print('generating...')
if not os.path.exists('benchmark/substr.env'):
    long_string('benchmark/substr.env')

if not os.path.exists('benchmark/substr-tree.env'):
    long_string('benchmark/substr-tree.env')

if not os.path.exists('benchmark/hash-string.env'):
    long_strings('benchmark/hash-string.env', 1000)

if not os.path.exists('benchmark/sum-tree.env'):
    large_tree('benchmark/sum-tree.env')

if not os.path.exists('benchmark/hash-tree.env'):
    large_tree('benchmark/hash-tree.env', 16)

if not os.path.exists('benchmark/pubkey-tree.env'):
    large_tree('benchmark/pubkey-tree.env', 10)

if not os.path.exists('benchmark/shift-left.env'):
    with open('benchmark/shift-left.env', 'w+') as f:
        f.write('(0xbadf00dfeedface 500)')

if not os.path.exists('benchmark/large-block.env'):
    generate_block('benchmark/large-block.env', p2_delegated_or_hidden_puzzle)

if not os.path.exists('benchmark/count-even.envhex'):
    generate_list('benchmark/count-even.envhex', 15000)

if not os.path.exists('benchmark/matrix-multiply.env'):
    size = 50
    with open('benchmark/matrix-multiply.env', 'w+') as f:
        f.write('(')
        for k in range(2):
            f.write('(')
            for i in range(size):
                f.write('(')
                for j in range(size):
                    f.write('%d ' % random.getrandbits(64))
                f.write(') ')
            f.write(') ')
        f.write(')')

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
test_costs = {}

native_opcode_names_by_opcode = dict(
    ("op_%s" % OP_REWRITE.get(k, k), op)
    for op, k in KEYWORD_FROM_ATOM.items()
    if k not in "qa."
)

print('benchmarking...')
for n in range(5):
    if "-v" in sys.argv:
        print('pass %d' % n)
    for fn in glob.glob('benchmark/*.hex'):
        env_fn = fn[:-3] + 'envhex'

        max_cost = 0
        flags = 0
        if '--brun' in sys.argv:
            command = ['brun', '-m', '100000000000', '-c', '--backend=rust', '--quiet', '--time', '--hex', fn, env_fn]
            if "-v" in sys.argv:
                print(" ".join(command))
            output = subprocess.check_output(command)
            output = output.decode('ascii').split('\n', 5)[:-1]
            cost = 0
        else:
            if "-v" in sys.argv:
                print(fn)
            program_data = bytes.fromhex(open(fn, 'r').read())
            env_data = bytes.fromhex(open(env_fn, 'r').read())

            time_start = time.perf_counter()
            cost, result = deserialize_and_run_program2(
                program_data,
                env_data,
                KEYWORD_TO_ATOM["q"][0],
                KEYWORD_TO_ATOM["a"][0],
                native_opcode_names_by_opcode,
                max_cost,
                flags,
            )
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
            test_costs[fn] = cost
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
    print(Fore.YELLOW + ' %13d' % test_costs[n], end='')
    print(Fore.RESET)

print(Fore.GREEN + '      TOTAL:' + Style.RESET_ALL + ' %f s' % sum_time)
print(Fore.GREEN + 'UNCERTAINTY:' + Style.RESET_ALL + ' %f s' % sum_uncertainty)
