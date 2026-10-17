#!/usr/bin/env python3
"""Independent pinned C SILK stage oracle; never links native Rust codec code.
Usage: stage_oracle.py --build <libopus OUT_DIR/build> --rate 12000 --channels 2 packet.bin [packet.bin ...]
Only temporary compiler artifacts are written; packets use one persistent decoder.
"""
import argparse
import hashlib
import json
import pathlib
import subprocess
import tempfile

REVISION = "34bba701ae97c913de719b1f7c10686f62cddb15"
p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--build", type=pathlib.Path, required=True)
p.add_argument("--rate", type=int, required=True)
p.add_argument("--channels", type=int, required=True)
p.add_argument("packets", type=pathlib.Path, nargs="+")
a = p.parse_args()
here = pathlib.Path(__file__).resolve().parent
source = here.parents[1] / "libopus/opus"
assert (
    subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    == REVISION
)
subprocess.run(
    ["git", "-C", str(source), "diff", "--exit-code", "HEAD", "--", "."],
    check=True,
    stdout=subprocess.DEVNULL,
)
build = a.build.resolve()
config = (build / "config.h").read_text()
assert "OPUS_FIXED_POINT:BOOL=ON" in (build / "CMakeCache.txt").read_text()
assert (
    "#define ENABLE_DEEP_PLC 1" not in config and "#define ENABLE_OSCE 1" not in config
)
archive = build / "libopus.a"
print(
    json.dumps(
        {
            "reference_revision": REVISION,
            "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
            "oracle_sha256": hashlib.sha256(
                (here / "stage_oracle.c").read_bytes()
            ).hexdigest(),
        }
    ),
    flush=True,
)
with tempfile.TemporaryDirectory(prefix="silk-oracle-") as temp:
    exe = pathlib.Path(temp) / "oracle"
    subprocess.run(
        [
            "cc",
            "-O1",
            "-DFIXED_POINT=1",
            "-DHAVE_CONFIG_H",
            *[
                "-I" + str(d)
                for d in [build, source / "include", source / "silk", source / "celt"]
            ],
            str(here / "stage_oracle.c"),
            str(archive),
            "-lm",
            "-Wl,--wrap=silk_decode_frame",
            "-Wl,--wrap=silk_stereo_decode_pred",
            "-Wl,--wrap=silk_stereo_MS_to_LR",
            "-Wl,--wrap=silk_decode_core",
            "-Wl,--wrap=silk_stereo_decode_mid_only",
            "-Wl,--wrap=silk_NLSF_stabilize",
            "-o",
            str(exe),
        ],
        check=True,
    )
    subprocess.run(
        [
            str(exe),
            str(a.rate),
            str(a.channels),
            *map(lambda f: str(f.resolve()), a.packets),
        ],
        check=True,
    )
