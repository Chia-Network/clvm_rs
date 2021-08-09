#!/usr/bin/env python3

from clvm_rs import run_generator2, STRICT_MODE
from time import time
from clvm_tools import binutils
import sys

def run_gen(fn, flags=0):

    # the generator ROM from:
    # https://github.com/Chia-Network/chia-blockchain/blob/main/chia/wallet/puzzles/rom_bootstrap_generator.clvm.hex
    program_data = bytes.fromhex(
        "ff02ffff01ff02ff0cffff04ff02ffff04ffff02ff05ffff04ff08ffff04ff13"
        "ff80808080ff80808080ffff04ffff01ffffff02ffff01ff05ffff02ff3effff"
        "04ff02ffff04ff05ff8080808080ffff04ffff01ffffff81ff7fff81df81bfff"
        "ffff02ffff03ffff09ff0bffff01818080ffff01ff04ff80ffff04ff05ff8080"
        "80ffff01ff02ffff03ffff0aff0bff1880ffff01ff02ff1affff04ff02ffff04"
        "ffff02ffff03ffff0aff0bff1c80ffff01ff02ffff03ffff0aff0bff1480ffff"
        "01ff0880ffff01ff04ffff0effff18ffff011fff0b80ffff0cff05ff80ffff01"
        "018080ffff04ffff0cff05ffff010180ff80808080ff0180ffff01ff04ffff18"
        "ffff013fff0b80ffff04ff05ff80808080ff0180ff80808080ffff01ff04ff0b"
        "ffff04ff05ff80808080ff018080ff0180ff04ffff0cff15ff80ff0980ffff04"
        "ffff0cff15ff0980ff808080ffff04ffff04ff05ff1380ffff04ff2bff808080"
        "ffff02ff16ffff04ff02ffff04ff09ffff04ffff02ff3effff04ff02ffff04ff"
        "15ff80808080ff8080808080ff02ffff03ffff09ffff0cff05ff80ffff010180"
        "ff1080ffff01ff02ff2effff04ff02ffff04ffff02ff3effff04ff02ffff04ff"
        "ff0cff05ffff010180ff80808080ff80808080ffff01ff02ff12ffff04ff02ff"
        "ff04ffff0cff05ffff010180ffff04ffff0cff05ff80ffff010180ff80808080"
        "8080ff0180ff018080ff04ffff02ff16ffff04ff02ffff04ff09ff80808080ff"
        "0d80ffff04ff09ffff04ffff02ff1effff04ff02ffff04ff15ff80808080ffff"
        "04ff2dffff04ffff02ff15ff5d80ff7d80808080ffff02ffff03ff05ffff01ff"
        "04ffff02ff0affff04ff02ffff04ff09ff80808080ffff02ff16ffff04ff02ff"
        "ff04ff0dff8080808080ff8080ff0180ff02ffff03ffff07ff0580ffff01ff0b"
        "ffff0102ffff02ff1effff04ff02ffff04ff09ff80808080ffff02ff1effff04"
        "ff02ffff04ff0dff8080808080ffff01ff0bffff0101ff058080ff0180ff0180"
        "80")

    # constants from the main chia blockchain:
    # https://github.com/Chia-Network/chia-blockchain/blob/main/chia/consensus/default_constants.py
    max_cost = 11000000000
    cost_per_byte = 12000

    env_data = binutils.assemble(open(fn, "r").read()).as_bin()

    # we don't charge for the size of the generator ROM. However, we do charge
    # cost for the operations it executes
    max_cost -= len(env_data) * cost_per_byte

    # add the block program arguments
    block_program_args = b"\xff\x80\x80"
    env_data = b"\xff" + env_data + b"\xff" + block_program_args  + b"\x80"

    try:
        return run_generator2(
            program_data,
            env_data,
            max_cost,
            flags,
        )
    except Exception as e:
        # GENERATOR_RUNTIME_ERROR
        return (117, None)


def print_spend_bundle_conditions(result) -> str:
    ret = ""
    if result.reserve_fee > 0:
        ret += f"RESERVE_FEE: {result.reserve_fee}\n"
    if result.height_absolute > 0:
        ret += f"ASSERT_HEIGHT_ABSOLUTE {result.height_absolute}\n"
    if result.seconds_absolute > 0:
        ret += f"ASSERT_SECONDS_ABSOLUTE {result.seconds_absolute}\n"
    for a in sorted(result.agg_sig_unsafe):
        ret += f"AGG_SIG_UNSAFE pk: {a[0].hex()} msg: {a[1].hex()}\n"
    ret += "SPENDS:\n"
    for s in sorted(result.spends, key=lambda x: x.coin_id):
        ret += f"- coin id: {s.coin_id.hex()} ph: {s.puzzle_hash.hex()}\n"

        if s.height_relative:
            ret += f"  ASSERT_HEIGHT_RELATIVE {s.height_relative}\n"
        if s.seconds_relative > 0:
            ret += f"  ASSERT_SECONDS_RELATIVE {s.seconds_relative}\n"
        for a in sorted(s.create_coin):
            if len(a[2]) > 0:
                ret += f"  CREATE_COIN: ph: {a[0].hex()} amount: {a[1]} hint: {a[2].hex()}\n"
            else:
                ret += f"  CREATE_COIN: ph: {a[0].hex()} amount: {a[1]}\n"
        for a in sorted(s.agg_sig_me):
            ret += f"  AGG_SIG_ME pk: {a[0].hex()} msg: {a[1].hex()}\n"
    ret += f"cost (clvm + conditions): {result.cost}\n"
    return ret


if __name__ == "__main__":
    try:
        start_time = time()
        error_code, result = run_gen(sys.argv[1])
        run_time = time() - start_time
        if error_code is not None:
            print(f"Validation Error: {error_code}")
            print(f"run-time: {run_time:.2f}s")
            sys.exit(1)
        start_time = time()
        print("Spend bundle:")
        print(print_spend_bundle_conditions(result))
        print_time = time() - start_time
        print(f"run-time: {run_time:.2f}s")
        print(f"print-time: {print_time:.2f}s")
    except Exception as e:
        run_time = time() - start_time
        print("FAIL:", e)
        print(f"run-time: {run_time:.2f}s")
