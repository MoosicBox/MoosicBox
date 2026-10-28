"""GitHub job-output sizing (UTF-16), including keys and duplicate aliases.

GitHub allows 1 MiB per job and 50 MiB per run. Reserve 64 KiB per
producer for unrelated outputs and bookkeeping; never treat UTF-8 as UTF-16.
The workflow guard checks the complete declared output mapping, not just a matrix.
"""

import json
import sys
from pathlib import Path

JOB_LIMIT = 1024 * 1024
HEADROOM = 64 * 1024
PAYLOAD = JOB_LIMIT - HEADROOM


def utf16_bytes(value):
    return len(value.encode("utf-16-le"))


def output_bytes(outputs):
    if not isinstance(outputs, dict) or any(
        not isinstance(key, str) or not isinstance(value, str)
        for key, value in outputs.items()
    ):
        raise ValueError("Output accounting requires a string-to-string mapping")
    return sum(utf16_bytes(key) + utf16_bytes(value) for key, value in outputs.items())


def validate_outputs(outputs):
    size = output_bytes(outputs)
    if size > PAYLOAD:
        raise ValueError(
            f"Combined job outputs use {size} UTF-16 bytes; maximum {PAYLOAD} "
            f"({JOB_LIMIT} GitHub limit minus {HEADROOM} reserved bytes)"
        )
    return size


if __name__ == "__main__":
    outputs = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    print(
        f"Combined job outputs: {validate_outputs(outputs)} UTF-16 bytes; "
        f"budget {PAYLOAD}, reserved {HEADROOM}"
    )
