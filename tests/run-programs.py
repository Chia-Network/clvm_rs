#!/usr/bin/env python3

import subprocess
import glob
import time
import sys
from clvm_rs import deserialize_and_run_program, STRICT_MODE
from clvm import KEYWORD_FROM_ATOM, KEYWORD_TO_ATOM
from clvm.operators import OP_REWRITE
from clvm.EvalError import EvalError
from colorama import init, Fore, Style

init()
ret = 0

native_opcode_names_by_opcode = dict(
    ("op_%s" % OP_REWRITE.get(k, k), op)
    for op, k in KEYWORD_FROM_ATOM.items()
    if k not in "qa."
)

for fn in glob.glob('programs/large-atom-*.hex.invalid'):

    try:
        program_data = bytes.fromhex(open(fn, 'r').read())
        max_cost = 40000000

        cost, result = deserialize_and_run_program(
            program_data,
            bytes.fromhex("ff80"),
            KEYWORD_TO_ATOM["q"][0],
            KEYWORD_TO_ATOM["a"][0],
            native_opcode_names_by_opcode,
            max_cost,
            0,
        )
        ret = 1
        print("FAILED: expected parse failure")
    except Exception as e:
        print("expected failure: %s" % e)


for fn in glob.glob('programs/*.clvm'):

    hexname = fn[:-4] + 'hex'
    with open(hexname, 'w+') as out:
        proc = subprocess.Popen(['opc', fn], stdout=out)
        proc.wait()

for fn in glob.glob('programs/*.env'):
    hexenv = fn + 'hex'
    with open(hexenv, 'w+') as out:
        proc = subprocess.Popen(['opc', fn], stdout=out)
        proc.wait()

for hexname in glob.glob('programs/*.hex'):

    hexenv = hexname[:-3] + 'envhex'

    if '--brun' in sys.argv:
        command = ['brun', '-m', '40000000', '-c', '--backend=rust', '--quiet', '--time', '--hex', hexname, hexenv]
        print(' '.join(command))
        start = time.perf_counter()
        subprocess.run(command)
        end = time.perf_counter()
    else:
        program_data = bytes.fromhex(open(hexname, 'r').read())
        env_data = bytes.fromhex(open(hexenv, 'r').read())

        print(f'{hexname} - ', end='')
        if len(program_data) == 0:
            print('  failed to compile')
            continue

        start = time.perf_counter()
        try:
            max_cost = 40000000

            cost, result = deserialize_and_run_program(
                program_data,
                env_data,
                KEYWORD_TO_ATOM["q"][0],
                KEYWORD_TO_ATOM["a"][0],
                native_opcode_names_by_opcode,
                max_cost,
                0,
            )
            end = time.perf_counter()
            print('{0:.2f}s'.format(end - start))

            ret += 1
            print(Fore.RED + '\nTEST FAILURE: expected to exceed cost' + Style.RESET_ALL)

        except EvalError as e:
            end = time.perf_counter()
            print('{0:.2f}s'.format(end - start))
            print(e)

    # cost 40000000 roughly corresponds to 4 seconds
    if end - start > 4.8:
        ret += 1
        print(Fore.RED + '\nTEST FAILURE: Time exceeded: %f\n' % (end - start) + Style.RESET_ALL)

if ret:
    print(Fore.RED + f'\n   There were {ret} failures!\n' + Style.RESET_ALL)

sys.exit(ret)
