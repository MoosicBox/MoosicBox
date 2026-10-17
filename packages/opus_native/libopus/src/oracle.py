"""Explicit-path client for the Cargo-linked reference_oracle example.

No auto-build, cache search, fixture writes, shared-library loading or silent skip.
"""

import json
from pathlib import Path
import subprocess


class Oracle:
    """One persistent encoder/decoder pair. Use as a context manager."""

    def __init__(self, executable):
        executable = str(Path(executable).resolve(strict=True))
        self.manifest = json.loads(subprocess.check_output([executable, "provenance"]))
        if self.manifest["schema"] != "moosicbox.opus.reference.v1":
            raise ValueError("unsupported reference manifest")
        self.process = subprocess.Popen(
            [executable],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            text=True,
            encoding="utf-8",
        )

    def request(self, op, **fields):
        """Send one bounded command; EOF/error is always a failed oracle run."""
        request = json.dumps(dict(op=op, **fields), separators=(",", ":")) + "\n"
        if len(request.encode("utf-8")) > 1048576:
            raise ValueError("oracle request exceeds 1 MiB")
        self.process.stdin.write(request)
        self.process.stdin.flush()
        response = self.process.stdout.readline(1048577)
        if not response or len(response.encode("utf-8")) > 1048576:
            raise RuntimeError("reference oracle failed or exceeded response bound")
        return json.loads(response)

    def __enter__(self):
        return self

    def __exit__(self, kind, value, traceback):
        self.process.stdin.close()
        self.process.stdout.close()
        status = self.process.wait()
        if status and kind is None:
            raise RuntimeError(f"reference oracle exited {status}")
