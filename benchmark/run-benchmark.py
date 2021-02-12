import glob
import subprocess
import sys
import os
import time
from clvm_rs import serialize_and_run_program, STRICT_MODE

procs = []

print('compiling...')
for fn in glob.glob('benchmark/*.clvm'):

    hex_name = fn[:-4] + 'hex'
    if not os.path.exists(hex_name):
        out = open(hex_name, 'w+')
        proc = subprocess.Popen(['opc', fn], stdout=out)
        procs.append(proc)

    env_hex_name = fn[:-4] + 'envhex'
    if not os.path.exists(env_hex_name):
        out = open(env_hex_name, 'w+')
        proc = subprocess.Popen(['opc', fn[:-4] + 'env'], stdout=out)
        procs.append(proc)

print("[" + (" " * len(procs)) + "]\r[", end="")
for p in procs:
    p.wait()
    print(".", end="")
    sys.stdout.flush()

print("")
test_runs = {}

for n in range(3):
    print('benchmarking, pass %d' % n)
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
for n, vals in test_runs.items():
    print('%20s:' % n, end='')
    mean = 0.0
    for v in vals:
        print(' %s' % v, end='')
        mean += float(v)
        sum_time += float(v)
    mean /= len(vals)

    diff = 0.0
    for v in vals:
        diff = max(abs(mean - float(v)), diff)
    print('   mean: %f (+/- %f)' % (mean, diff))
    sum_uncertainty += diff

print('TOTAL: %f s' % sum_time)
print('UNCERTAINTY: %f s' % sum_uncertainty)
