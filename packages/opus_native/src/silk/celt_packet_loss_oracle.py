"""Public CELT loss/recovery oracle. Explicit runner only; no build or cache lookup."""

import math
import os
from pathlib import Path
import struct
import sys

root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / "libopus/src"))
from oracle import Oracle

coded, n, burst, channels = map(int, sys.argv[2:6])
short = len(sys.argv) > 9 and sys.argv[9] == "short"
rate = int(sys.argv[6]) if len(sys.argv) > 6 else 48000
api_n = n * rate // 48000
with Oracle(os.environ["MOOSICBOX_OPUS_REFERENCE_ORACLE"]) as oracle:

    def checked(op, **fields):
        result = oracle.request(op, **fields)
        assert "error" not in result, result
        return result

    hybrid = len(sys.argv) > 7 and sys.argv[7] == "hybrid"
    silk = len(sys.argv) > 7 and sys.argv[7] == "silk"
    checked(
        "encoder",
        rate=48000,
        channels=coded,
        application=2049 if hybrid else 2048 if silk else 2051,
    )
    checked("decoder", rate=rate, channels=channels)
    checked("configure", controls=[[4002, 128000], [4010, 0]])
    if silk:
        bandwidth = int(sys.argv[8]) if len(sys.argv) > 8 else 1103
        checked(
            "configure",
            controls=[
                [11002, 1000],
                [4004, bandwidth],
                [4008, bandwidth],
                [4002, 32000],
            ],
        )
    if hybrid:
        checked(
            "configure",
            controls=[[11002, 1001], [4004, 1105], [4008, 1105], [4002, 48000]],
        )
    for index in range(2 * (8 + burst)):
        step = index % (8 + burst)
        if index == 8 + burst:
            checked("reset", state="encoder")
            checked("reset", state="decoder")
        lost = 5 <= step < 5 + burst
        packet = []
        if not lost:
            pcm = [
                int(
                    7000
                    * math.sin(2 * math.pi * (220 + ch * 113) * (step * n + i) / 48000)
                    + 1900 * math.sin(2 * math.pi * 731 * (step * n + i) / 48000)
                )
                for i in range(n)
                for ch in range(coded)
            ]
            packet = checked("encode", frame_size=n, pcm=pcm)["packet"]
            assert packet and (
                12 <= packet[0] >> 3 < 16
                if hybrid
                else packet[0] >> 3 < 12 if silk else packet[0] >> 3 >= 16
            )
        output = checked(
            "decode",
            packet=packet if packet else None,
            frame_size=rate // 200 if short and lost else api_n,
        )
        assert output["samples"] == (rate // 200 if short and lost else api_n)
        sys.stdout.buffer.write(
            struct.pack("<II", len(packet), len(output["pcm"])) + bytes(packet)
        )
        sys.stdout.buffer.write(
            struct.pack("<" + "h" * len(output["pcm"]), *output["pcm"])
        )
