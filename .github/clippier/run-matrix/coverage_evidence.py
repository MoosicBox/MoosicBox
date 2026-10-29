"""Fail-closed evidence checks for configurations with no executable tests."""

import json
import re
import sys
from pathlib import Path


def classify(text):
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    errors = [line.strip() for line in text.splitlines() if "error:" in line]
    missing = [
        line
        for line in errors
        if (
            "error: failed to merge profile data: not found *.profraw files in " in line
            or (
                "error: failed to load coverage:" in line
                and line.endswith("no coverage data found")
            )
        )
    ]
    old_missing = "warning: not found *.profraw files in " in text
    if old_missing:
        missing.extend(
            line
            for line in errors
            if (
                line.startswith("error: no input files specified.")
                or line.startswith(
                    "error: failed to merge profile data: process didn't exit successfully:"
                )
            )
        )
    if not missing:
        raise ValueError("Not a recognized absent-coverage failure")
    for line in errors:
        if line in missing:
            continue
        if (
            line == "error: could not load coverage information"
            and "no coverage data found" in text
        ):
            continue
        if (
            line.startswith(
                "error: failed to generate report: process didn't exit successfully:"
            )
            and "no coverage data found" in text
        ):
            continue
        raise ValueError("Unexpected error alongside absent coverage")
    # LLVM can print object errors without cargo-llvm-cov's `error:` prefix.
    if re.search(
        r"corrupt|malformed|unsupported.*version|mismatched|hash mismatch", text, re.I
    ):
        raise ValueError("Invalid coverage data is not empty coverage")


def verify(text):
    finished = False
    binaries = 0
    summaries = 0
    for line in text.splitlines():
        if not line.strip():
            continue
        if line.startswith("{"):
            value = json.loads(line)
            reason = value.get("reason")
            if reason == "build-finished":
                if value.get("success") is not True:
                    raise ValueError("Test enumeration build failed")
                finished = True
            elif reason == "compiler-artifact":
                if value.get("profile", {}).get("test") and value.get("executable"):
                    binaries += 1
            elif reason != "compiler-message":
                raise ValueError("Unknown cargo message during test enumeration")
        elif re.fullmatch(r"0 tests?, 0 benchmarks?", line):
            summaries += 1
        else:
            raise ValueError("Nonempty or unrecognized test enumeration")
    if not finished or binaries != summaries:
        raise ValueError("Missing successful enumeration evidence for each test binary")


if __name__ == "__main__":
    command, filename = sys.argv[1:]
    {"classify": classify, "verify": verify}[command](
        Path(filename).read_text(encoding="utf-8")
    )
