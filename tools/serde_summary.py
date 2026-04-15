#!/usr/bin/env python3
"""Compare legacy, backref, and 2026 serialization formats on .generator files."""

import glob
import sys
import time

from clvm_rs.serde import (
    deser_backrefs,
    deser_legacy,
    deser_2026,
    ser_backrefs,
    ser_legacy,
    ser_2026,
)

ITERATIONS = 10


def bench(fn, *args, iterations=ITERATIONS):
    """Return (result, microseconds_per_call)."""
    result = fn(*args)
    start = time.perf_counter()
    for _ in range(iterations):
        fn(*args)
    elapsed = (time.perf_counter() - start) / iterations * 1e6
    return result, elapsed


def main():
    files = sys.argv[1:] or sorted(glob.glob("benches/*.generator"))
    if not files:
        print("No .generator files found.", file=sys.stderr)
        sys.exit(1)

    iterations = ITERATIONS

    for path in files:
        raw = open(path, "rb").read()

        # .generator files are backref-compressed; inflate to get a LazyNode
        node = deser_backrefs(raw)

        # try to get legacy bytes (may be too large)
        try:
            legacy_bytes = ser_legacy(node)
        except Exception:
            legacy_bytes = None

        if legacy_bytes is not None:
            node = deser_legacy(legacy_bytes)

        results = []

        # --- Legacy ---
        if legacy_bytes is not None:
            _, ser_us = bench(ser_legacy, node, iterations=iterations)
            serialized = ser_legacy(node)
            _, deser_us = bench(deser_legacy, serialized, iterations=iterations)
            results.append(("legacy", len(serialized), ser_us, deser_us))

        # --- Backrefs ---
        _, ser_us = bench(ser_backrefs, node, iterations=iterations)
        serialized = ser_backrefs(node)
        _, deser_us = bench(deser_backrefs, serialized, iterations=iterations)
        results.append(("backrefs", len(serialized), ser_us, deser_us))

        # --- 2026 ---
        _, ser_us = bench(ser_2026, node, iterations=iterations)
        serialized = ser_2026(node)
        _, deser_us = bench(deser_2026, serialized, iterations=iterations)
        results.append(("2026", len(serialized), ser_us, deser_us))

        print(f"=== {path} ===")
        print(f"  {'format':>10}  {'ser (µs)':>12}  {'deser (µs)':>12}  {'size':>10}")
        for name, size, ser_us, deser_us in results:
            print(f"  {name:>10}  {ser_us:>12.1f}  {deser_us:>12.1f}  {size:>10}")

        legacy_size = next((s for n, s, _, _ in results if n == "legacy"), None)
        if legacy_size is not None:
            print()
            print("  size vs legacy:")
            for name, size, _, _ in results:
                if name != "legacy":
                    ratio = size / legacy_size * 100
                    print(f"    {name}: {ratio:.1f}%")
        print()


if __name__ == "__main__":
    main()
