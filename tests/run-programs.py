#!/usr/bin/env python3

import subprocess
import glob
import time
import sys
import platform
from colorama import init, Fore, Style
from run import run_clvm

init()
ret = 0

for fn in glob.glob('programs/large-atom-*.hex.invalid'):

    try:
        run_clvm(fn)
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

for hexname in sorted(glob.glob('programs/*.hex')):

    hexenv = hexname[:-3] + 'envhex'

#    command = ['brun', '-m', '11000000000', '-c', '--backend=rust', '--quiet', '--time', '--hex', hexname, hexenv]
    command = ['./run.py', hexname, hexenv]

    # prepend the size command, to measure RSS
    if platform.system() == 'Darwin':
        command = ['/usr/bin/time', '-l'] + command;
    if platform.system() == 'Linux':
        command = ['/usr/bin/time'] + command;

    print(' '.join(command))
    start = time.perf_counter()
    proc = subprocess.run(command, stderr=subprocess.PIPE, stdout=subprocess.PIPE)
    output = proc.stderr.decode('UTF-8')
    output += proc.stdout.decode('UTF-8')
    end = time.perf_counter()

    if 'FAIL: cost exceeded' not in output:
        ret += 1
        print(Fore.RED + '\nTEST FAILURE: expected cost to be exceeded\n' + Style.RESET_ALL)
        print(output)

    print(Fore.YELLOW + ('  Runtime: %0.2f s' % (end - start)) + Style.RESET_ALL)

    # parse RSS (MacOS and Linux only)
    size = None
    if platform.system() == 'Darwin':
        for l in output.split('\n'):
            if 'maximum resident set size' not in l:
                continue
            val, key = l.strip().split('  ', 1)
            if key == 'maximum resident set size':
                size = int(val) / 1024 / 1024
    if platform.system() == 'Linux':
        # example output:
        # 10.49user 0.32system 0:10.84elapsed 99%CPU (0avgtext+0avgdata 189920maxresident)k
        for l in output.split('\n'):
            if 'maxresident)k' not in l:
                continue
            size = int(l.split('maxresident)k')[0].split(' ')[-1]) / 1024
    if size != None:
        print(Fore.YELLOW + ('  Resident Size: %d MiB' % size) + Style.RESET_ALL)

        if size > 2300:
            ret += 1
            print(Fore.RED + '\nTEST FAILURE: Max memory use exceeded (limit: 2300 MB)\n' + Style.RESET_ALL)

    # cost 10923314721 roughly corresponds to 11 seconds
    if end - start > 11:
        ret += 1
        print(Fore.RED + '\nTEST FAILURE: Time exceeded: %f (limit: 11)\n' % (end - start) + Style.RESET_ALL)

if ret:
    print(Fore.RED + f'\n   There were {ret} failures!\n' + Style.RESET_ALL)

sys.exit(ret)
