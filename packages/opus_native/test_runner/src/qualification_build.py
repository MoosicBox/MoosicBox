"""Capture a coherent release build receipt around an actual allocated Cargo build.

Run only with the coordinator's global build slot. Never rebuilds C, discovers a
cache image or issues a retrospective receipt. Existing receipt paths reject.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from qualification_matrix import native_source_identity


def sha256(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def capture(kind, target_directory, receipt_path, reference=None):
    root = Path(__file__).resolve().parents[4]
    target_directory = Path(target_directory).resolve()
    receipt_path = Path(receipt_path).resolve()
    if receipt_path.exists():
        raise FileExistsError(
            "receipt already exists; do not overwrite retained evidence"
        )
    if kind == "matrix" and reference is None:
        raise ValueError("matrix build requires the explicit immutable reference image")
    reference = Path(reference).resolve(strict=True) if reference else None
    reference_before = sha256(reference) if reference else None
    example = "native_oracle" if kind == "matrix" else "qualification_performance"
    command = [
        "cargo",
        "build",
        "--locked",
        "--offline",
        "--release",
        "--target-dir",
        str(target_directory),
        "-p",
        "moosicbox_opus_native_test_runner",
        "--example",
        example,
        "--message-format=json",
    ]
    environment = dict(os.environ, CARGO_BUILD_JOBS="2")
    # These are observations around the actual command, never hashes generated
    # after an unrelated pre-existing image was built.
    before = native_source_identity()
    compiler = subprocess.check_output(
        ["rustc", "-Vv"], cwd=root, env=environment, text=True
    )
    started = time.time_ns()
    result = subprocess.run(
        command, cwd=root, env=environment, capture_output=True, text=True
    )
    finished = time.time_ns()
    sys.stderr.write(result.stderr)
    after = native_source_identity()
    artifacts = []
    for line in result.stdout.splitlines():
        message = json.loads(line)
        if (
            message.get("reason") == "compiler-artifact"
            and message.get("target", {}).get("name") == example
            and "example" in message.get("target", {}).get("kind", [])
            and message.get("executable")
        ):
            artifacts.append(message)
    if result.returncode:
        raise RuntimeError(
            f"actual Cargo build failed with status {result.returncode}; no receipt issued"
        )
    if before != after:
        raise ValueError("source changed during build; no coherent receipt issued")
    if len(artifacts) != 1:
        raise ValueError(
            "Cargo did not identify exactly one requested example executable"
        )
    artifact = artifacts[0]
    profile = artifact.get("profile", {})
    if (
        profile.get("debug_assertions") is not False
        or str(profile.get("opt_level")) == "0"
    ):
        raise ValueError("Cargo artifact is not an optimized release image")
    executable = Path(artifact["executable"]).resolve(strict=True)
    if not executable.is_relative_to(target_directory):
        raise ValueError("Cargo artifact is outside the explicit target directory")
    image_hash = sha256(executable)
    if reference and sha256(reference) != reference_before:
        raise ValueError("reference image changed during native build")
    receipt = dict(
        schema=(
            "moosicbox.opus.qualification-build.v1"
            if kind == "matrix"
            else "moosicbox.opus.performance-build.v1"
        ),
        source_before_sha256=before,
        source_after_sha256=after,
        command=command,
        profile="release",
        rustc_version=compiler,
        build_started_unix_ns=started,
        build_finished_unix_ns=finished,
        cargo_profile=profile,
        executable=str(executable),
        fresh=artifact.get("fresh"),
    )
    if kind == "matrix":
        receipt.update(native_sha256=image_hash, reference_sha256=reference_before)
    else:
        receipt["executable_sha256"] = image_hash
    with receipt_path.open("x", encoding="utf-8") as output:
        output.write(json.dumps(receipt, sort_keys=True, indent=2) + "\n")
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--kind", choices=("matrix", "performance"), required=True)
    parser.add_argument("--target-directory", type=Path, required=True)
    parser.add_argument("--receipt", type=Path, required=True)
    parser.add_argument("--reference", type=Path)
    args = parser.parse_args()
    receipt = capture(args.kind, args.target_directory, args.receipt, args.reference)
    print(json.dumps(dict(receipt=str(args.receipt), executable=receipt["executable"])))


if __name__ == "__main__":
    main()
