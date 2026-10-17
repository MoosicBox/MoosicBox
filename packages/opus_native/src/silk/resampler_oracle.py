#!/usr/bin/env python3
"""Verify frozen SILK resampler output using the pinned independent C archive.
Usage: resampler_oracle.py --build target/debug/build/<libopus-id>/out/build
Never updates expected data. Regeneration requires separately reviewed source edits.
"""
import argparse
import hashlib
import json
import pathlib
import subprocess
import tempfile

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--build", type=pathlib.Path, required=True)
a = p.parse_args()
here = pathlib.Path(__file__).resolve().parent
source = here.parents[1] / "libopus/opus"
revision = "34bba701ae97c913de719b1f7c10686f62cddb15"
assert (
    subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    == revision
)
subprocess.run(
    ["git", "-C", str(source), "diff", "--exit-code", "HEAD", "--", "."],
    check=True,
    stdout=subprocess.DEVNULL,
)
build = a.build.resolve()
cache = (build / "CMakeCache.txt").read_text()
for setting in [
    "OPUS_FIXED_POINT:BOOL=ON",
    "OPUS_DISABLE_INTRINSICS:BOOL=ON",
    "OPUS_ENABLE_FLOAT_API:BOOL=OFF",
]:
    assert setting in cache, setting
expected_provenance = json.loads(
    (here / "reference/resampler_provenance.json").read_text()
)
assert (
    hashlib.sha256((here / "resampler_oracle.c").read_bytes()).hexdigest()
    == expected_provenance["oracle_sha256"]
)
with tempfile.TemporaryDirectory(prefix="silk-resampler-oracle-") as temp:
    executable = pathlib.Path(temp) / "oracle"
    command = [
        "cc",
        "-O1",
        "-DFIXED_POINT=1",
        "-DHAVE_CONFIG_H",
        *[
            "-I" + str(d)
            for d in [build, source / "include", source / "silk", source / "celt"]
        ],
        str(here / "resampler_oracle.c"),
        str(build / "libopus.a"),
        "-lm",
        "-o",
        str(executable),
    ]
    subprocess.run(command, check=True)
    actual = subprocess.check_output([str(executable)])
    expected = (here / "reference/resampler.pcm").read_bytes()
    assert (
        actual == expected
    ), "Reference resampler drift: do not automatically update expected PCM"
    assert hashlib.sha256(actual).hexdigest() == expected_provenance["pcm_sha256"]
    print(
        json.dumps(
            {
                "revision": revision,
                "bytes": len(actual),
                "pcm_sha256": hashlib.sha256(actual).hexdigest(),
                "archive_sha256": hashlib.sha256(
                    (build / "libopus.a").read_bytes()
                ).hexdigest(),
            }
        )
    )
