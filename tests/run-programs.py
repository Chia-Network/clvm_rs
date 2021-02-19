#!/usr/bin/env python3

import subprocess
import glob
import time
import sys
from clvm_rs import deserialize_and_run_program, STRICT_MODE
from clvm import KEYWORD_FROM_ATOM, KEYWORD_TO_ATOM
from clvm.operators import OP_REWRITE

ret = 0

native_opcode_names_by_opcode = dict(
    ("op_%s" % OP_REWRITE.get(k, k), op)
    for op, k in KEYWORD_FROM_ATOM.items()
    if k not in "qa."
)

for fn in glob.glob('programs/large-atom-*.hex'):

    try:
        program_data = bytes.fromhex(open(fn, 'r').read())
        max_cost = 100000

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

    env = fn[:-4] + 'env'
    hexenv = fn[:-4] + 'envhex'
    with open(hexenv, 'w+') as out:
        proc = subprocess.Popen(['opc', env], stdout=out)
        proc.wait()

    command = ['brun', '-m', '10000', '-c', '--backend=rust', '--quiet', '--time', '--hex', hexname, hexenv]
    print(' '.join(command))
    start = time.perf_counter()
    subprocess.run(command)
    end = time.perf_counter()
    if end - start > 1:
        ret = 1
        print('Time exceeded: %f' % (end - start))

sys.exit(ret)
