"""Offline release codec measurements; no C, media, downloads or Cargo invocation.

Explicit executable and coherent build receipt required. Results are local
measurements, not CI/platform acceptance, quality certification or budget gates.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time

from qualification_matrix import native_source_identity

CASES = (
    "celt-mono",
    "celt-stereo",
    "celt-highrate",
    "silk-mono",
    "hybrid-stereo",
    "auto-stereo",
)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def verify_receipt(path, source, executable):
    receipt = json.loads(Path(path).read_text(encoding="utf-8"))
    expected = dict(
        schema="moosicbox.opus.performance-build.v1",
        source_before_sha256=source,
        source_after_sha256=source,
        executable_sha256=executable,
        profile="release",
    )
    if any(receipt.get(key) != value for key, value in expected.items()):
        raise ValueError("performance receipt does not match release source/image")
    if not receipt.get("command") or not receipt.get("rustc_version"):
        raise ValueError("receipt requires actual build command and compiler identity")
    return receipt


def measure(executable, case, frames, timeout):
    # Fresh child per case prevents prior cases contaminating peak RSS. File-backed
    # output avoids a pipe deadlock while wait4 collects this exact child's usage.
    with tempfile.TemporaryFile() as output, tempfile.TemporaryFile() as errors:
        started = time.perf_counter()
        child = subprocess.Popen(
            [str(executable), case, str(frames)], stdout=output, stderr=errors
        )
        peak_bytes = None
        peak_method = "unavailable on this platform"
        try:
            if hasattr(os, "wait4") and platform.system() in ("Linux", "Darwin"):
                while True:
                    pid, status, usage = os.wait4(child.pid, os.WNOHANG)
                    if pid:
                        child.returncode = os.waitstatus_to_exitcode(status)
                        peak_bytes = usage.ru_maxrss * (
                            1024 if platform.system() == "Linux" else 1
                        )
                        peak_method = (
                            "wait4 child ru_maxrss; includes codec, harness and runtime"
                        )
                        break
                    if time.perf_counter() - started > timeout:
                        raise subprocess.TimeoutExpired(child.args, timeout)
                    time.sleep(0.01)
            else:
                child.wait(timeout=timeout)
        except BaseException:
            child.kill()
            child.wait()
            raise
        wall = time.perf_counter() - started
        if child.returncode:
            errors.seek(0)
            raise RuntimeError(
                f"{case} exited {child.returncode}: "
                + errors.read(4096).decode(errors="replace")
            )
        output.seek(0)
        raw = output.read(65537)
        if len(raw) > 65536:
            raise ValueError("benchmark output exceeded 64 KiB")
        row = json.loads(raw)
        if (
            row.get("schema") != "moosicbox.opus.performance-case.v1"
            or row.get("debug_assertions") is not False
            or row.get("case") != case
            or row.get("frames") != frames
        ):
            raise ValueError("unexpected benchmark case/release identity")
        audio_ns = row["audio_ns"]
        if audio_ns != frames * 20_000_000:
            raise ValueError("unexpected benchmark audio duration")
        for phase in ("encode", "decode"):
            total = row[phase]["total_ns"]
            if total <= 0:
                raise ValueError("nonpositive codec timing")
            row[phase]["audio_realtime_multiple"] = audio_ns / total
            row[phase]["frames_per_second"] = frames * 1_000_000_000 / total
        row.update(
            peak_rss_bytes=peak_bytes,
            peak_rss_method=peak_method,
            process_wall_seconds=wall,
        )
        return row


def cpu_identity():
    # CPU model is an observation, not a portable hardware equivalence claim.
    path = Path("/proc/cpuinfo")
    if platform.system() == "Linux" and path.exists():
        for line in path.read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    return platform.processor() or "unavailable"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--build-receipt", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--frames", type=int, default=500)
    parser.add_argument("--timeout-seconds", type=int, default=300)
    parser.add_argument("--repetitions", type=int, default=2)
    args = parser.parse_args()
    if (
        not 10 <= args.frames <= 30000
        or not 1 <= args.repetitions <= 10
        or not 1 <= args.timeout_seconds <= 3600
    ):
        parser.error("frames 10..30000, repetitions 1..10, timeout 1..3600 required")
    executable = args.executable.resolve(strict=True)
    source = native_source_identity()
    image = digest(executable)
    receipt = verify_receipt(args.build_receipt, source, image)
    report = dict(
        schema="moosicbox.opus.performance.v1",
        executable_sha256=image,
        source_before_sha256=source,
        build_receipt=receipt,
        script_sha256=digest(__file__),
        cases=[],
        measurement_complete=False,
        platform=dict(
            system=platform.system(),
            release=platform.release(),
            machine=platform.machine(),
            cpu=cpu_identity(),
            logical_cpus=os.cpu_count(),
            python=platform.python_version(),
        ),
        limitation="local measurement only; synthetic PCM is not quality evidence; no CI, other-OS or performance-budget acceptance",
    )
    try:
        for case in CASES:
            first = None
            for repetition in range(args.repetitions):
                row = measure(executable, case, args.frames, args.timeout_seconds)
                identities = tuple(
                    row[key] for key in ("input_sha256", "packets_sha256", "pcm_sha256")
                )
                if first is not None and first != identities:
                    raise ValueError(f"nondeterministic benchmark output: {case}")
                first = identities
                row["repetition"] = repetition
                report["cases"].append(row)
        report["source_after_sha256"] = native_source_identity()
        report["artifact_stable"] = digest(executable) == image
        if report["source_after_sha256"] != source or not report["artifact_stable"]:
            raise ValueError("source/image changed during performance measurement")
        report["measurement_complete"] = True
        report["peak_memory_complete"] = all(
            row["peak_rss_bytes"] is not None for row in report["cases"]
        )
    except Exception as error:
        report["error"] = str(error)[:1024]
        raise
    finally:
        args.output.write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    print(
        json.dumps(
            dict(
                output=str(args.output),
                sha256=digest(args.output),
                measurement_complete=True,
                peak_memory_complete=report["peak_memory_complete"],
            )
        )
    )


if __name__ == "__main__":
    main()
