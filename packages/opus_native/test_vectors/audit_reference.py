#!/usr/bin/env python3
"""Offline independent frozen-corpus re-decode. Never writes fixture expectations."""
import argparse
import hashlib
import json
import pathlib
import subprocess
import tempfile

ARCHIVE = "4f2a11b2264e7172169207a29a35e624bda97fdfbd3e2cdb4026bff658b4d500"
REVISION = "34bba701ae97c913de719b1f7c10686f62cddb15"
MANIFEST = "8361b967ee0ac2fc4385e3b52d46dafd96921077aea3b23928b45f3d6251e8c3"
C = r"""
#include <opus.h>
#include <stdio.h>
#include <stdlib.h>
int main(int argc, char **argv) {
    if (argc < 4) return 1;
    int error, channels = atoi(argv[2]);
    OpusDecoder *decoder = opus_decoder_create(atoi(argv[1]), channels, &error);
    if (!decoder || error) return 2;
    for (int i = 3; i < argc; ++i) {
        unsigned char packet[65536]; opus_int16 pcm[5760*2];
        FILE *f = fopen(argv[i], "rb"); if (!f) return 3;
        size_t size = fread(packet, 1, sizeof(packet), f);
        if (ferror(f) || !size || !feof(f)) return 4;
        fclose(f);
        int n = opus_decode(decoder, packet, (opus_int32)size, pcm, 5760, 0);
        if (n <= 0) return 5;
        for (int j = 0; j < n * channels; ++j) {
            unsigned short s = (unsigned short)pcm[j];
            if (putchar(s & 255) == EOF || putchar(s >> 8) == EOF) return 6;
        }
    }
    opus_decoder_destroy(decoder);
    return 0;
}
"""


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--build",
        required=True,
        type=pathlib.Path,
        help="Pinned CMake build directory containing libopus.a and CMakeCache.txt",
    )
    args = parser.parse_args()
    here = pathlib.Path(__file__).resolve().parent
    source = here.parent / "libopus/opus"
    build = args.build.resolve()
    archive = build / "libopus.a"
    if sha(archive.read_bytes()) != ARCHIVE:
        raise ValueError("Reference archive SHA-256 mismatch; no fallback")
    cache = (build / "CMakeCache.txt").read_text()
    for setting in [
        "OPUS_FIXED_POINT:BOOL=ON",
        "OPUS_DISABLE_INTRINSICS:BOOL=ON",
        "OPUS_ENABLE_FLOAT_API:BOOL=OFF",
        "OPUS_BUILD_SHARED_LIBRARY:BOOL=OFF",
    ]:
        if setting not in cache:
            raise ValueError("Reference build configuration mismatch: " + setting)
    config = (build / "config.h").read_text()
    for forbidden in ["#define ENABLE_DEEP_PLC 1", "#define ENABLE_OSCE 1"]:
        if forbidden in config:
            raise ValueError("Unexpected reference neural enhancement: " + forbidden)
    actual = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    if actual != REVISION:
        raise ValueError("Reference source revision mismatch")
    subprocess.run(
        ["git", "-C", str(source), "diff", "--quiet", "HEAD", "--", "."], check=True
    )
    raw = (here / "frozen/manifest.json").read_bytes()
    if sha(raw) != MANIFEST:
        raise ValueError("Frozen manifest SHA-256 mismatch")
    manifest = json.loads(raw)
    corpus = here / "frozen/corpus"
    for relative, entry in manifest["files"].items():
        path = pathlib.PurePosixPath(relative)
        if path.is_absolute() or ".." in path.parts or "\\" in relative:
            raise ValueError("Unsafe manifest path")
        data = (corpus / relative).read_bytes()
        if len(data) != entry["bytes"] or sha(data) != entry["sha256"]:
            raise ValueError("Frozen file changed: " + relative)
    packets = samples = 0
    with tempfile.TemporaryDirectory(prefix="opus-frozen-audit-") as temporary:
        temp = pathlib.Path(temporary)
        c = temp / "audit.c"
        c.write_text(C)
        executable = temp / "audit"
        subprocess.run(
            [
                "cc",
                "-O2",
                "-I" + str(source / "include"),
                str(c),
                str(archive),
                "-lm",
                "-o",
                str(executable),
            ],
            check=True,
        )
        for case in manifest["cases"]:
            count = case["packet_count"]
            if count <= 0:
                raise ValueError("Empty mandatory case")
            root = corpus / case["path"]
            dirs = [root if i == 0 else root / f"packet_{i:03}" for i in range(count)]
            meta = [json.loads((d / "meta.json").read_bytes()) for d in dirs]
            if any(
                m["sample_rate"] != meta[0]["sample_rate"]
                or m["channels"] != meta[0]["channels"]
                for m in meta
            ):
                raise ValueError("Unexpected sequence format transition")
            expected = b"".join((d / "expected.pcm").read_bytes() for d in dirs)
            if len(expected) != sum(
                m["frame_samples"] * m["channels"] * 2 for m in meta
            ):
                raise ValueError("Incomplete raw PCM")
            actual = subprocess.check_output(
                [
                    str(executable),
                    str(meta[0]["sample_rate"]),
                    str(meta[0]["channels"]),
                    *[str(d / "packet.bin") for d in dirs],
                ]
            )
            if actual != expected:
                raise ValueError("Independent C raw PCM mismatch: " + case["id"])
            packets += count
            samples += len(actual) // 2
    if len(manifest["cases"]) != 43 or packets != 58:
        raise ValueError("Incomplete corpus")
    print(
        json.dumps(
            {
                "cases": 43,
                "packets": packets,
                "interleaved_samples": samples,
                "raw_pcm_exact": True,
                "archive_sha256": ARCHIVE,
                "manifest_sha256": MANIFEST,
                "audit_script_sha256": sha(pathlib.Path(__file__).read_bytes()),
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
