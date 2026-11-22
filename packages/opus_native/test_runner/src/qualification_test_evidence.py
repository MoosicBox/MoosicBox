"""Capture real test images and libtest output for the finish verifier."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

import qualification_matrix_acceptance as verify

ci = verify.ci
PIPELINES = (*ci.PRODUCT_TESTS,
             ("moosicbox_audio_output", "opus-native", "opus_native_pipeline"),
             ("moosicbox_files", "files,encoder-opus-native", "opus_pipeline"))


def write(path, data):
    with path.open("x", encoding="utf-8") as f:
        json.dump(data, f, indent=2, sort_keys=True)
        f.write("\n")


def ref(root, path):
    return {"path": path.relative_to(root).as_posix(), "sha256": verify.digest(path)}


def sources():
    files = {p.relative_to(ci.ROOT).as_posix(): verify.digest(p)
             for p in sorted((ci.ROOT / "packages/files").rglob("*.rs"))}
    # Match the verifier's native path rendering for the files manifest.
    files = {str(Path(k)): v for k, v in files.items()}
    return verify.matrix.native_source_identity(), ci.sources(), files


def run(argv, log, env, timeout, cwd):
    timed_out = False
    with log.open("xb") as out:
        try:
            result = subprocess.run(argv, cwd=cwd, env=env, stdout=out,
                                    stderr=subprocess.STDOUT, timeout=timeout).returncode
        except subprocess.TimeoutExpired:
            result, timed_out = None, True
    status = dict(command=list(map(str, argv)), exit_code=result, timed_out=timed_out)
    write(log.with_suffix(".command.json"), status)
    if result != 0:
        raise RuntimeError(f"command failed or timed out; inspect {log}")
    return status


def capture(root, directory, profile, package, features, target, env, timeout):
    directory.mkdir(parents=True, exist_ok=False)
    unit = target is None
    target_name = package if unit else target
    argv = ["cargo", "test", "--locked", "--offline", "-j2", "-p", package,
            "--no-default-features", "--features", features,
            *(["--lib"] if unit else ["--test", target]),
            *(["--release"] if profile == "release" else []),
            "--no-run", "--message-format=json"]
    before = sources()
    build = directory / "build.log"
    run(argv, build, env, timeout, ci.ROOT)
    candidates = []
    for line in build.read_bytes().splitlines():
        if not line.startswith(b'{'):
            continue
        obj = json.loads(line)
        if (obj.get("reason") == "compiler-artifact" and obj.get("executable")
                and obj["profile"]["test"] and obj["target"]["name"] == target_name
                and ("lib" if unit else "test") in obj["target"]["kind"]):
            candidates.append(Path(obj["executable"]))
    if len(candidates) != 1:
        raise ValueError(f"expected exactly one test image: {candidates}")
    built, = candidates
    image = directory / built.name
    shutil.copy2(built, image)
    identity = verify.digest(image)
    after = sources()
    if identity != verify.digest(built) or before != after:
        raise ValueError("source or test image changed during build")
    receipt = dict(source_before_sha256=before[0], source_after_sha256=after[0],
                   executable_sha256=identity, command=argv, profile=profile,
                   build_log=ref(root, build))
    package_dir = ci.ROOT / "packages" / package.removeprefix("moosicbox_")
    listings = {}
    names = []
    for prefix, flags in (("list", []), ("ignored_list", ["--ignored"])):
        log = directory / (prefix + ".log")
        run([image, "--list", *flags], log, env, timeout, package_dir)
        listed = re.findall(r"^(.+): test$", log.read_text(encoding="utf-8"), re.M)
        if len(listed) != len(set(listed)):
            raise ValueError("duplicate libtest names")
        names.append(set(listed))
        listings.update({prefix + "_log": ref(root, log)["path"],
                         prefix + "_sha256": verify.digest(log)})
    all_names, ignored = names
    ordinary = sorted(all_names - ignored)
    if not ordinary or not ignored <= all_names:
        raise ValueError("empty or inconsistent inventory")
    inventory = directory / "inventory.json"
    write(inventory, dict(ordinary_tests=ordinary, ignored_tests=sorted(ignored), **listings))
    log = directory / "suite.log"
    status = run([image, "--test-threads=1", "--show-output"], log, env, timeout, package_dir)
    suite = dict(**status, log=ref(root, log)["path"], log_sha256=verify.digest(log))
    verify.verify_full_suite(root, suite, ordinary, ignored)
    ci.reject_partial(log.read_text(encoding="utf-8"))
    if verify.digest(image) != identity or before != sources():
        raise ValueError("source or retained image changed during execution")
    evidence = dict(executable_file=ref(root, image)["path"], executable_sha256=identity,
                    build_receipt=receipt, inventory=ref(root, inventory), full_suite=suite)
    if not unit:
        evidence.update(profile=profile, package=package, features=features, target=target, **suite)
    write(directory / "evidence.json", evidence)
    return evidence


def produce(root, kind, timeout):
    source, manifest, files = initial = sources()
    directory = root / kind
    directory.mkdir(parents=True, exist_ok=False)
    profiles, suites = {}, []
    # Never inherit a potentially stale Cargo target or reuse someone else's image.
    with tempfile.TemporaryDirectory(prefix="opus-target-") as temp:
        env = ci.acceptance_environment(dict(os.environ, CARGO_TARGET_DIR=temp))
        for profile in ("release", "debug"):
            if kind == "units":
                profiles[profile] = capture(root, directory/profile, profile, "moosicbox_opus_native",
                                            ci.FEATURES, None, env, timeout)
            else:
                for package, features, target in PIPELINES:
                    suites.append(capture(root, directory/profile/target, profile, package,
                                          features, target, env, timeout))
    if sources() != initial:
        raise ValueError("source changed between profiles")
    if kind == "units":
        report = dict(schema="moosicbox.opus.unit-shards.v1", source_sha256=source,
                      test_source_manifest=manifest, profiles=profiles)
    else:
        report = dict(schema="moosicbox.opus.pipeline-evidence.v1", source_sha256=manifest,
                      files_test_sources=files, suites=suites)
    candidate = directory / "candidate.json"
    write(candidate, report)
    if kind == "units":
        verify.verify_unit_shards(root, ref(root, candidate), source)
    else:
        verify.verify_pipeline(root, ref(root, candidate))
    candidate.rename(directory / "report.json")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kind", choices=("units", "pipeline"))
    parser.add_argument("root", type=Path)
    parser.add_argument("--timeout", type=int, default=14400, help="per-command seconds")
    args = parser.parse_args()
    produce(args.root.resolve(), args.kind, args.timeout)
