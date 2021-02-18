#!/usr/bin/env python

import subprocess
import glob
import time
import sys

ret = 0

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
