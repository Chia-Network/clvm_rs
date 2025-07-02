#!/usr/bin/env python3

import subprocess
import glob
import time
import sys
import platform
from colorama import init, Fore, Style
from run import run_clvm
from os.path import isfile, splitext, basename

init()
ret = 0

expect = {
    "args-add": "Cost Exceeded",
    "args-all": "Cost Exceeded",
    "args-and": "Cost Exceeded",
    "args-any": "Cost Exceeded",
    "args-cat": "Cost Exceeded",
    "args-mul": "Cost Exceeded",
    "args-or": "Cost Exceeded",
    "args-point_add": "Cost Exceeded",
    "args-sha": "Cost Exceeded",
    "args-sub": "Cost Exceeded",
    "args-unknown-1": "Cost Exceeded",
    "args-unknown-2": "Cost Exceeded",
    "args-unknown-3": "Cost Exceeded",
    "args-unknown-4": "Cost Exceeded",
    "args-unknown-5": "Too Many Pairs",
    "args-unknown-6": "Too Many Pairs",
    "args-unknown-7": "Too Many Pairs",
    "args-unknown-8": "Too Many Pairs",
    "args-unknown-9": "Too Many Pairs",
    "args-xor": "Cost Exceeded",
    "recursive-add": "Cost Exceeded",
    "recursive-ash": "Cost Exceeded",
    "recursive-cat": "Cost Exceeded",
    "recursive-cons": "Too Many Pairs",
    "recursive-div": "Cost Exceeded",
    "recursive-lsh": "Cost Exceeded",
    "recursive-mul": "Cost Exceeded",
    "recursive-not": "Cost Exceeded",
    "recursive-pubkey": "Cost Exceeded",
    "recursive-sub": "Too Many Pairs",
    "softfork-1": "Cost Exceeded",
    "softfork-2": "Cost Exceeded",
}

for fn in glob.glob("programs/large-atom-*.hex.invalid"):
    try:
        print(fn)
        run_clvm(fn)
        ret = 1
        print(Fore.RED + "FAILED: expected parse failure" + Style.RESET_ALL)
    except Exception as e:
        print(Fore.GREEN + f"OK: expected: {e}" + Style.RESET_ALL)


for fn in glob.glob("programs/*.clvm"):
    hexname = fn[:-4] + "hex"
    if isfile(hexname):
        continue
    with open(hexname, "w+") as out:
        print(f"compiling {fn}")
        proc = subprocess.Popen(["opc", fn], stdout=out)
        proc.wait()

for fn in glob.glob("programs/*.env"):
    hexenv = fn + "hex"
    if isfile(hexenv):
        continue
    with open(hexenv, "w+") as out:
        print(f"compiling {fn}")
        proc = subprocess.Popen(["opc", fn], stdout=out)
        proc.wait()

for hexname in sorted(glob.glob("programs/*.hex")):
    hexenv = hexname[:-3] + "envhex"

    command = ["./run.py", hexname, hexenv]

    # prepend the size command, to measure RSS
    if platform.system() == "Darwin":
        command = ["/usr/bin/time", "-l"] + command
    if platform.system() == "Linux":
        command = ["/usr/bin/time"] + command

    print(" ".join(command))
    start = time.perf_counter()
    proc = subprocess.run(command, stderr=subprocess.PIPE, stdout=subprocess.PIPE)
    output = proc.stderr.decode("UTF-8")
    output += proc.stdout.decode("UTF-8")
    end = time.perf_counter()

    test = splitext(basename(hexname))[0]
    expected_error = expect[test]
    if f"FAIL: {expected_error}" not in output:
        ret += 1
        print(
            Fore.RED
            + f'\nTEST FAILURE: expected "{expected_error}"\n'
            + Style.RESET_ALL
        )
        print(output)

    print(Fore.YELLOW + ("  Runtime: %0.2f s" % (end - start)) + Style.RESET_ALL)

    # parse RSS (MacOS and Linux only)
    size = None
    if platform.system() == "Darwin":
        for l in output.split("\n"):
            if "maximum resident set size" not in l:
                continue
            val, key = l.strip().split("  ", 1)
            if key == "maximum resident set size":
                size = int(val) / 1024 / 1024
    if platform.system() == "Linux":
        # example output:
        # 10.49user 0.32system 0:10.84elapsed 99%CPU (0avgtext+0avgdata 189920maxresident)k
        for l in output.split("\n"):
            if "maxresident)k" not in l:
                continue
            size = int(l.split("maxresident)k")[0].split(" ")[-1]) / 1024
    if size != None:
        print(Fore.YELLOW + ("  Resident Size: %d MiB" % size) + Style.RESET_ALL)

        if size > 2300:
            ret += 1
            print(
                Fore.RED
                + "\nTEST FAILURE: Max memory use exceeded (limit: 2300 MB)\n"
                + Style.RESET_ALL
            )

    # cost 10923314721 roughly corresponds to 11 seconds
    if end - start > 11:
        ret += 1
        print(
            Fore.RED
            + "\nTEST FAILURE: Time exceeded: %f (limit: 11)\n" % (end - start)
            + Style.RESET_ALL
        )

if ret:
    print(Fore.RED + f"\n   There were {ret} failures!\n" + Style.RESET_ALL)

sys.exit(ret)
