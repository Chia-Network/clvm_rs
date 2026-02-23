#!/usr/bin/env python3
"""
Run generate_program with incrementing seeds, then brun, and count successes vs failures.

Usage (from repo root):
  cargo build --release -p clvm-fuzzing --bin generate_program   # once
  python3 run_generate_and_brun.py

Requires: cargo, brun (from clvm_tools_rs) on PATH.
"""

import heapq
import random
import subprocess
import sys
from pathlib import Path

PROGRAM_HEX = "program.hex"
ENV_HEX = "env.hex"
NUM_RUNS = 1000

_CANONICAL_ERRORS = (
    "* requires int args",
    "+ requires int args",
    "- requires int args",
    "/ requires int args",
    "= on list",
    "> requires int args",
    ">s on list",
    "Division by zero",
    "Environment Stack Limit Reached",
    "G1 atom on list",
    "G2 atom on list",
    "Internal Error: ",
    "Invalid Nil Terminator in operand list",
    "InvalidAllocatorArg: ",
    "InvalidOperatorArg: ",
    "Out of Memory",
    "Reserved operator",
    "Secp256 Verify Error: failed",
    "Shift too large",
    "Too Many Atoms",
    "Value Stack Limit Reached",
    "ash requires int args",
    "ash requires int32 args",
    "atom is not G1 size, 48 bytes",
    "atom is not G2 size, 96 bytes",
    "atom is not a G1 point",
    "atom is not a G2 point",
    "atom is not a valid G1 point",
    "atom is not a valid G2 point",
    "bad encoding",
    "bad operand list",
    "bls_pairing_identity failed",
    "bls_verify failed",
    "bls_verify message on list",
    "clvm raise",
    "clvm raise",
    "coinid on list",
    "coinid: invalid amount (may not be negative",
    "coinid: invalid amount (may not exceed max coin amount)",
    "coinid: invalid parent coin id (must be 32 bytes)",
    "coinid: invalid puzzle hash (must be 32 bytes)",
    "concat on list",
    "cost exceeded or below zero",
    "cost must be > 0",
    "div with 0",
    "divmod requires int args",
    "divmod with 0",
    "first of non-cons",
    "g1_map on list",
    "g1_map takes exactly 1 or 2 arguments",
    "g1_multiply requires int args",
    "g2_map on list",
    "g2_multiply requires int args",
    "in ((X)...) syntax X must be lone atom",
    "in the ((X)...) syntax, the inner list takes exactly 1 argument",
    "invalid backreference during deserialisation",
    "invalid indices for substr",
    "invalid operator",
    "keccak256 on list",
    "logand requires int args",
    "logior requires int args",
    "lognot requires int args",
    "logxor requires int args",
    "lsh on list",
    "lsh requires int args",
    "lsh requires int32 args",
    "mod requires int args",
    "mod with 0",
    "modpow requires int args",
    "modpow with 0 modulus",
    "modpow with negative exponent",
    "pair found, expected G1 point",
    "pair found, expected G2 point",
    "path into atom",
    "pubkey_for_exp requires int args",
    "rest of non-cons",
    "secp256k1_verify pubkey is not valid",
    "secp256k1_verify pubkey on list",
    "secp256r1_verify pubkey is not valid",
    "secp256r1_verify pubkey on list",
    "sha256 on list",
    "shift too large",
    "softfork requires int arg",
    "softfork requires positive int arg",
    "softfork requires u32 arg",
    "softfork requires u64 arg",
    "softfork specified cost mismatch",
    "strlen requires an atom",
    "substr requires an atom",
    "substr requires int32 args",
    "too many pairs",
    "unimplemented operator",
    "unknown softfork extension",
    "unknown softfork extension",
)


def canonical_brun_error(stderr: str) -> str:
    msg = stderr.split("FAIL: ", 1)[1].strip()
    for canonical in _CANONICAL_ERRORS:
        if msg.startswith(canonical):
            return canonical
    return msg

def parse_brun_timings(stdout: str) -> int:
    lines = stdout.splitlines()
    read_hex_val = int(lines[0].split("read_hex:", 1)[1].strip())
    run_program_val = int(lines[1].split("run_program:", 1)[1].strip())
    return read_hex_val + run_program_val


def main():
    success_count = 0
    fail_count = 0
    # Histogram of error message -> count (for failed runs)
    error_histogram: dict[str, int] = {}
    # Min-heap of up to 50 (-total_time, seed) so largest time is at top; negate back when presenting
    longest_runtimes: list[tuple[int, int]] = []

    seed_offset = random.getrandbits(32)
    for i in range(NUM_RUNS):
        # Generate program and env
        seed = i + seed_offset
        gen_cmd = [
            "cargo", "run", "--release",
            "-p", "clvm-fuzzing",
            "--bin", "generate_program",
            str(seed),
            PROGRAM_HEX,
            ENV_HEX,
        ]
        result = subprocess.run(
            gen_cmd,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print(f"Seed {seed}: generate_program failed: {result.stderr or result.stdout}", file=sys.stderr)
            sys.exit(1)

        # Run brun
        brun_cmd = ["brun", "--time", "-x", "--quiet", PROGRAM_HEX, ENV_HEX]
        result = subprocess.run(
            brun_cmd,
            capture_output=True,
            text=True,
        )
        stdout = result.stdout or ""
        if result.returncode != 0 or "FAIL: " in stdout:
            msg = canonical_brun_error(stdout)
            error_histogram[msg] = error_histogram.get(msg, 0) + 1
            print(f"{i} ({seed}) -> FAIL")
            fail_count += 1
        else:
            total_time = parse_brun_timings(stdout)
            heapq.heappush(longest_runtimes, (-total_time, seed))
            print(f"{i} {seed} -> SUCCESS")
            success_count += 1

    print(f"Runs: {NUM_RUNS}")
    print(f"Successful: {success_count}")
    print(f"Failed: {fail_count}")

    if error_histogram:
        print("\nError message histogram (most common first):")
        for msg, count in sorted(error_histogram.items(), key=lambda x: -x[1]):
            print(f"  {count:6d}  {msg}")

    if longest_runtimes:
        print("\n10 longest total run times (read_hex + run_program), longest first:")
        for neg_time, s in heapq.nsmallest(10, longest_runtimes):
            print(f"  {-neg_time}  seed={s}")


if __name__ == "__main__":
    main()
