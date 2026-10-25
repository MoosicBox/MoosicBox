"""Serial, resumable 192-case internal SILK candidate validation.

Uses an explicit already-built release unit-test executable, never Cargo. Source
and compiler identities are observations at execution time, NOT build provenance.
Internal candidate evidence does not qualify public guards or perceptual quality.
"""

import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import subprocess
import time
import uuid

from qualification_inputs import MANIFEST, verify_inputs, verify_reference

TEST = "encoder::silk::public::internal_practical_floor_media_matrix"
SCHEMA = "moosicbox.opus.silk-candidates.v1"
ROOT = Path(__file__).resolve().parents[4]


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def cases():
    return tuple(
        f"{media}/{rate}/{channels}/{milliseconds}/{mono_bitrate * channels}/{scale}"
        for media, rate, channels, milliseconds, mono_bitrate, scale in itertools.product(
            ("speech", "music"),
            (8000, 12000, 16000),
            (1, 2),
            (10, 20, 40, 60),
            (16000, 32000),
            ("1", "0.01"),
        )
    )


def source_identity():
    # Include the unit-test source, wrappers and runner dependencies, not just HEAD.
    root = ROOT / "packages/opus_native"
    paths = list((root / "src").rglob("*.rs"))
    paths += list((root / "libopus/src").glob("*.rs"))
    paths += list((root / "test_runner/src").glob("qualification*.py"))
    paths += [
        ROOT / "Cargo.toml",
        ROOT / "Cargo.lock",
        root / "Cargo.toml",
        root / "libopus/Cargo.toml",
        root / "test_runner/Cargo.toml",
        MANIFEST,
    ]
    h = hashlib.sha256()
    for path in sorted(set(paths)):
        h.update(str(path.relative_to(ROOT)).encode() + b"\0")
        h.update(path.read_bytes())
    return h.hexdigest()


def observation(executable, reference, media_directory):
    manifest, media = verify_inputs(media_directory)
    reference_bytes = digest(reference)
    provenance = json.loads(
        subprocess.check_output([str(reference), "provenance"], timeout=30)
    )
    verify_reference(provenance, manifest)
    if digest(reference) != reference_bytes:
        raise ValueError("reference changed during provenance verification")
    compiler_command = shutil.which("rustc")
    if not compiler_command:
        raise FileNotFoundError("rustc observation tool unavailable")
    # Hash the invoked tool before following any rustup/config dispatch. The -Vv
    # output identifies the observed compiler; neither establishes image provenance.
    compiler_command = str(Path(compiler_command).absolute())
    compiler_tool_hash = digest(compiler_command)
    compiler = subprocess.check_output([compiler_command, "-Vv"], timeout=15).decode()
    if digest(compiler_command) != compiler_tool_hash:
        raise ValueError("rustc observation tool changed during invocation")
    identity = dict(
        source_sha256=source_identity(),
        executable_sha256=digest(executable),
        reference_sha256=reference_bytes,
        reference_provenance=provenance,
        media={
            label: dict(path=str(path), sha256=digest(path))
            for label, path in media.items()
        },
        input_manifest_sha256=digest(MANIFEST),
        runner_sha256=digest(__file__),
        rustc_observed=compiler,
        rustc_command=compiler_command,
        rustc_tool_sha256=compiler_tool_hash,
        rustc_observed_sha256=hashlib.sha256(compiler.encode()).hexdigest(),
        test=TEST,
        command=[str(executable), TEST, "--exact", "--ignored", "--nocapture"],
        platform=dict(
            system=platform.system(),
            release=platform.release(),
            machine=platform.machine(),
        ),
        identity_scope="source/compiler observed now; no claim executable was built from this snapshot",
    )
    return identity


def log_passes(path, case):
    text = Path(path).read_text(encoding="utf-8", errors="replace")
    # libtest may print its name/prefix on the same line as the first stderr marker.
    prefix = "test " + TEST + " ... "
    named_success = (
        len(
            re.findall(
                r"^test " + re.escape(TEST) + r" \.\.\. ok\s*$", text, re.MULTILINE
            )
        )
        == 1
    )
    interleaved_success = (
        text.count(prefix) == 1 and len(re.findall(r"^ok\s*$", text, re.MULTILINE)) == 1
    )
    text = text.replace(prefix, "")
    starts = re.findall(r"^internal SILK CASE START (.+)$", text, re.MULTILINE)
    # Test-name success alone is insufficient: prove exactly one internal case ran.
    return (
        starts == [case]
        and len(
            re.findall(
                r"^internal SILK matrix executed=1 failures=0$", text, re.MULTILINE
            )
        )
        == 1
        and len(re.findall(r"^running 1 test\s*$", text, re.MULTILINE)) == 1
        and (named_success or interleaved_success)
        and len(
            re.findall(
                r"^test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured;",
                text,
                re.MULTILINE,
            )
        )
        == 1
    )


def reusable(attempt, case, directory, provenance_id):
    try:
        name = attempt["log"]
        if Path(name).name != name:
            return False
        path = directory / name
        return (
            attempt["case"] == case
            and attempt["status"] == "pass"
            and attempt["exit_code"] == 0
            and attempt["timed_out"] is False
            and attempt["provenance_sha256"] == provenance_id
            and digest(path) == attempt["log_sha256"]
            and log_passes(path, case)
        )
    except (KeyError, OSError, TypeError, ValueError):
        return False


def provenance_hash(identity):
    return hashlib.sha256(
        json.dumps(identity, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def save(path, report):
    temporary = path.with_name(path.name + "." + uuid.uuid4().hex + ".partial")
    with temporary.open("x", encoding="utf-8") as output:
        output.write(json.dumps(report, sort_keys=True, indent=2) + "\n")
        output.flush()
        os.fsync(output.fileno())
    os.replace(temporary, path)


def terminate_tree(child):
    """Reap the test and its persistent C oracle descendants after timeout."""
    if os.name == "posix":
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
    elif os.name == "nt":
        # Windows process groups do not themselves provide tree termination.
        subprocess.run(
            ["taskkill", "/PID", str(child.pid), "/T", "/F"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=5,
            check=False,
        )
    if child.poll() is None:
        child.kill()
    child.wait(timeout=5)


def run_case(
    executable, reference, media_directory, case, directory, timeout, provenance_id
):
    log = directory / (
        "case-"
        + hashlib.sha256(case.encode()).hexdigest()[:16]
        + "-"
        + uuid.uuid4().hex
        + ".log"
    )
    environment = dict(os.environ)
    environment.update(
        MOOSICBOX_OPUS_SILK_CASE=case,
        MOOSICBOX_OPUS_REFERENCE_ORACLE=str(reference),
        MOOSICBOX_OPUS_QUALIFICATION_MEDIA=str(media_directory),
        RUST_TEST_THREADS="1",
        RUST_TEST_NOCAPTURE="1",
    )
    command = [str(executable), TEST, "--exact", "--ignored", "--nocapture"]
    started = time.monotonic()
    code = None
    timed_out = False
    error = None
    with log.open("xb") as output:
        try:
            process_options = dict(start_new_session=True) if os.name == "posix" else {}
            child = subprocess.Popen(
                command,
                cwd=ROOT,
                env=environment,
                stdout=output,
                stderr=subprocess.STDOUT,
                **process_options,
            )
            try:
                code = child.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                timed_out = True
                terminate_tree(child)
                code = child.returncode
            except BaseException:
                terminate_tree(child)
                raise
        except OSError as failure:
            error = str(failure)
    status = (
        "timeout"
        if timed_out
        else "pass" if code == 0 and log_passes(log, case) else "failure"
    )
    return dict(
        case=case,
        status=status,
        exit_code=code,
        timed_out=timed_out,
        elapsed_seconds=time.monotonic() - started,
        log=log.name,
        log_sha256=digest(log),
        provenance_sha256=provenance_id,
        error=error,
    )


def summarize(report, directory, provenance_id):
    passed = [
        case
        for case in cases()
        if any(
            reusable(attempt, case, directory, provenance_id)
            for attempt in report["attempts"]
            if attempt.get("case") == case
        )
    ]
    report.update(
        required_cases=192,
        verified_passes=len(passed),
        missing_cases=[case for case in cases() if case not in passed],
        complete=len(passed) == 192 and report.get("provenance_stable") is True,
    )


def run(
    executable, reference, media_directory, directory, selected, timeout, resume=False
):
    selected = tuple(selected) if selected else cases()
    if len(set(selected)) != len(selected) or set(selected) - set(cases()):
        raise ValueError(
            "filters must be unique exact members of the 192-case enumeration"
        )
    executable = Path(executable).resolve(strict=True)
    reference = Path(reference).resolve(strict=True)
    media_directory = Path(media_directory).resolve(strict=True)
    directory = Path(directory).resolve()
    identity = observation(executable, reference, media_directory)
    provenance_id = provenance_hash(identity)
    if resume:
        report = json.loads((directory / "report.json").read_text(encoding="utf-8"))
        if (
            report.get("schema") != SCHEMA
            or report.get("identity") != identity
            or report.get("provenance_sha256") != provenance_id
        ):
            raise ValueError(
                "resume provenance mismatch; retain old evidence and use a new directory"
            )
    else:
        directory.mkdir(exist_ok=False)
        report = dict(
            schema=SCHEMA,
            identity=identity,
            provenance_sha256=provenance_id,
            attempts=[],
            complete=False,
            limitation="internal candidate case validation only; not public floor qualification or perceptual certification",
        )
    report.update(
        selected_cases=list(selected), provenance_stable=False, complete=False
    )
    save(directory / "report.json", report)
    for case in selected:
        if not any(
            reusable(attempt, case, directory, provenance_id)
            for attempt in report["attempts"]
        ):
            attempt = run_case(
                executable,
                reference,
                media_directory,
                case,
                directory,
                timeout,
                provenance_id,
            )
            report["attempts"].append(attempt)
        report["provenance_stable"] = False
        summarize(report, directory, provenance_id)
        save(directory / "report.json", report)
        # Mutated identities invalidate completion even for successful individual tests.
        try:
            stable = observation(executable, reference, media_directory) == identity
        except Exception as error:
            report["provenance_error"] = str(error)[:1024]
            save(directory / "report.json", report)
            raise
        report["provenance_stable"] = stable
        summarize(report, directory, provenance_id)
        save(directory / "report.json", report)
        if not stable:
            raise ValueError(
                "source/image/reference/media/tool provenance changed during run"
            )
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--executable",
        type=Path,
        required=True,
        help="explicit prebuilt release unit-test image",
    )
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--media-directory", type=Path, required=True)
    parser.add_argument("--output-directory", type=Path, required=True)
    parser.add_argument(
        "--cases",
        action="append",
        help="repeat for exact focused filters; complete still requires all 192",
    )
    parser.add_argument("--timeout-seconds", type=int, default=300)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    if not 1 <= args.timeout_seconds <= 3600:
        parser.error("timeout must be 1..3600 seconds")
    report = run(
        args.executable,
        args.reference,
        args.media_directory,
        args.output_directory,
        args.cases,
        args.timeout_seconds,
        args.resume,
    )
    print(
        json.dumps(
            dict(
                report=str(args.output_directory / "report.json"),
                complete=report["complete"],
                verified_passes=report["verified_passes"],
                required_cases=192,
            )
        )
    )
    return 0 if report["complete"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
