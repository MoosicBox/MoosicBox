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
    "silk-mono-48k",
    "silk-stereo-96k",
    "hybrid-stereo-32k",
    "hybrid-stereo-64k",
    "celt-stereo-32k",
    "celt-stereo-64k",
    "auto-stereo-32k",
    "auto-stereo-64k",
)

# Below the current standalone SILK public floor; never silently count as covered.
CANDIDATE_CASES = ("silk-mono-16k", "silk-mono-32k")


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


def performance_verdict(report):
    """Local >=1x codec-throughput gate; no invented headroom or memory budget."""
    failures = []
    rows = report.get("cases", [])
    repetitions = report.get("repetitions")
    if (
        not isinstance(repetitions, int)
        or isinstance(repetitions, bool)
        or repetitions < 2
    ):
        return dict(
            accepted=False,
            failures=["missing repeated-run coverage"],
            minimum_audio_realtime_multiple=1.0,
        )
    expected = {
        (case, repetition) for case in CASES for repetition in range(repetitions)
    }
    seen = set()
    identities = {}
    for row in rows:
        key = (row.get("case"), row.get("repetition"))
        if key in seen:
            failures.append(f"duplicate case/repetition: {key}")
        seen.add(key)
        if key not in expected:
            failures.append(f"non-public or unexpected performance case: {key}")
        frames, audio = row.get("frames"), row.get("audio_ns")
        integer = (
            lambda value: isinstance(value, int)
            and not isinstance(value, bool)
            and value > 0
        )
        if (
            not integer(frames)
            or frames != report.get("frames")
            or not integer(audio)
            or audio != frames * 20_000_000
        ):
            failures.append(f"invalid duration: {key}")
            continue
        for phase in ("encode", "decode"):
            timing = row.get(phase, {})
            total, median, p95, maximum = (
                timing.get(field)
                for field in ("total_ns", "median_ns", "p95_ns", "max_ns")
            )
            if (
                not all(integer(value) for value in (total, median, p95, maximum))
                or not median <= p95 <= maximum <= total
                or total > frames * maximum
            ):
                failures.append(f"invalid latency statistics: {key} {phase}")
            elif total > audio:
                failures.append(f"below real-time: {key} {phase}")
        hashes = tuple(
            row.get(field) for field in ("input_sha256", "packets_sha256", "pcm_sha256")
        )
        if any(
            not isinstance(value, str)
            or len(value) != 64
            or any(c not in "0123456789abcdef" for c in value)
            for value in hashes
        ):
            failures.append(f"missing output identities: {key}")
        if row.get("case") in identities and identities[row["case"]] != hashes:
            failures.append(f"nonreproducible output: {key}")
        identities[row.get("case")] = hashes
        if row.get("debug_assertions") is not False:
            failures.append(f"not a release image: {key}")
    if seen != expected:
        failures.append("incomplete 14-case repeated public performance coverage")
    if (
        report.get("measurement_complete") is not True
        or report.get("artifact_stable") is not True
    ):
        failures.append("measurement incomplete or image unstable")
    if not report.get("source_before_sha256") or report.get(
        "source_before_sha256"
    ) != report.get("source_after_sha256"):
        failures.append("source identity missing or changed")
    return dict(
        accepted=not failures,
        failures=failures,
        minimum_audio_realtime_multiple=1.0,
        latency_policy="total encode and decode each <= measured audio duration; median/p95/max retained, no headroom budget approved",
        memory_policy="raw child high-water RSS only; launch/pre-exec floor may dominate; not isolated codec allocation proof",
    )


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
    parser.add_argument(
        "--candidate-cases",
        action="store_true",
        help="also run below-public-floor candidates; rejection remains a failure",
    )
    args = parser.parse_args()
    if (
        not 10 <= args.frames <= 30000
        or not 2 <= args.repetitions <= 10
        or not 1 <= args.timeout_seconds <= 3600
    ):
        parser.error("frames 10..30000, repetitions 2..10, timeout 1..3600 required")
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
        frames=args.frames,
        repetitions=args.repetitions,
        performance_accepted=False,
        public_envelope_only=not args.candidate_cases,
        required_qualification_blockers=[
            dict(
                case=case,
                status="below_current_public_silk_floor",
                qualified=False,
                performance_measured=False,
            )
            for case in CANDIDATE_CASES
        ],
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
        for case in CASES + (CANDIDATE_CASES if args.candidate_cases else ()):
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
        report["performance_verdict"] = performance_verdict(report)
        report["performance_accepted"] = report["performance_verdict"]["accepted"]
        args.output.write_text(
            json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8"
        )
    print(
        json.dumps(
            dict(
                output=str(args.output),
                sha256=digest(args.output),
                measurement_complete=True,
                performance_accepted=report["performance_accepted"],
                peak_memory_complete=report["peak_memory_complete"],
            )
        )
    )
    return 0 if report["performance_accepted"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
