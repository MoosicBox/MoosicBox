#!/usr/bin/env python3
"""Measure cold clippier-md process startup separately from Criterion benchmarks."""

from __future__ import annotations

import argparse
import json
import platform
import statistics
import subprocess
import tempfile
import time
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--samples", type=int, default=20)
    args = parser.parse_args()

    binary = args.binary.resolve()
    if not binary.is_file():
        raise SystemExit(f"binary does not exist: {binary}")

    samples_ms: list[float] = []
    with tempfile.TemporaryDirectory(prefix="clippier-md-startup-") as directory:
        fixture = Path(directory) / "canonical.md"
        fixture.write_text("# Canonical\n\nA short canonical paragraph.\n")
        for _ in range(args.samples):
            started = time.perf_counter_ns()
            subprocess.run(
                [str(binary), "fmt", "--check", "--no-diff", str(fixture)],
                check=True,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            samples_ms.append((time.perf_counter_ns() - started) / 1_000_000)

    samples_ms.sort()
    report = {
        "benchmark": "cold_process_startup",
        "binary": str(binary),
        "samples": len(samples_ms),
        "median_ms": statistics.median(samples_ms),
        "min_ms": samples_ms[0],
        "max_ms": samples_ms[-1],
        "platform": platform.platform(),
        "python": platform.python_version(),
    }
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
